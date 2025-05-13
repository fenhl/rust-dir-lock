//! This is `dir-lock`, a library crate providing the type [`DirLock`], which is a simple file-system-based mutex.

use {
    std::{
        io,
        mem::forget,
        num::ParseIntError,
        path::{
            Path,
            PathBuf,
        },
        sync::Arc,
        thread,
        time::Duration,
    },
    sysinfo::{
        Pid,
        ProcessRefreshKind,
        ProcessesToUpdate,
    },
    thiserror::Error,
    tokio::{
        fs,
        time::sleep,
    },
};

/// A simple file-system-based mutex.
///
/// When constructing a value of this type, a directory is created at the specified path.
/// If a directory already exists, the constructor waits until it's removed.
/// Dropping a `DirLock` removes the corresponding directory.
/// Since creating a directory if it does not exist is an atomic operation on most operating systems,
/// this can be used as a quick-and-dirty cross-process mutex.
///
/// To guard against processes exiting without properly removing the lock, a file containing the current process ID is created inside the lock.
/// If no process with that ID exists, another process may claim the lock for itself.
/// If the file does not exist, the constructor waits until it does (or until the directory is removed).
///
/// Of course, this is still not completely fail-proof since the user or other processes could mess with the lock directory.
///
/// This type is a RAII lock guard, but unlocking a directory lock uses I/O and can error, so it is recommended to call [`drop_async`](Self::drop_async).
#[must_use = "should call the drop_async method to unlock"]
pub struct DirLock(PathBuf);

/// An error that can occur when locking or unlocking a [`DirLock`].
#[derive(Debug, Error, Clone)]
#[allow(missing_docs)]
pub enum Error {
    #[error("I/O error{}: {}", if let Some(path) = .1 { format!(" at {}", path.display()) } else { String::default() }, .0)] Io(#[source] Arc<io::Error>, Option<PathBuf>),
    #[error(transparent)] ParseInt(#[from] ParseIntError),
}

trait IoResultExt {
    type T;

    fn at(self, path: impl AsRef<Path>) -> Self::T;
}

impl IoResultExt for io::Error {
    type T = Error;

    fn at(self, path: impl AsRef<Path>) -> Error {
        Error::Io(Arc::new(self), Some(path.as_ref().to_owned()))
    }
}

impl<T, E: IoResultExt> IoResultExt for Result<T, E> {
    type T = Result<T, E::T>;

    fn at(self, path: impl AsRef<Path>) -> Result<T, E::T> {
        self.map_err(|e| e.at(path))
    }
}

impl DirLock {
    /// Acquires a directory lock at the given path, without blocking the thread.
    ///
    /// See the type-level docs for details.
    pub async fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        loop {
            match fs::create_dir(&path).await {
                Ok(()) => {
                    let pidfile = path.join("pid");
                    fs::write(&pidfile, format!("{}\n", std::process::id())).await.at(pidfile)?;
                    return Ok(Self(path))
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        let pidfile = path.join("pid");
                        if match fs::read_to_string(&pidfile).await {
                            Ok(buf) => {
                                !buf.is_empty() // assume pidfile is still being written if empty //TODO check timestamp
                                && !pid_exists(buf.trim().parse()?)
                            }
                            Err(e) => if e.kind() == io::ErrorKind::NotFound {
                                false
                            } else {
                                return Err(e.at(path.join("pid")))
                            },
                        } {
                            clean_up_path(&path).await?;
                        }
                        sleep(Duration::from_secs(1)).await;
                        continue
                    }
                    _ => return Err(e.at(path)),
                },
            }
        }
    }

    /// Blocks the current thread until the lock can be established.
    pub fn new_sync(path: &impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref().to_owned();
        loop {
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let pidfile = path.join("pid");
                    std::fs::write(&pidfile, format!("{}\n", std::process::id())).at(pidfile)?;
                    return Ok(Self(path))
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        let pidfile = path.join("pid");
                        if match std::fs::read_to_string(&pidfile) {
                            Ok(buf) => {
                                !buf.is_empty() // assume pidfile is still being written if empty //TODO check timestamp
                                && !pid_exists(buf.trim().parse()?)
                            }
                            Err(e) => if e.kind() == io::ErrorKind::NotFound {
                                false
                            } else {
                                return Err(e.at(path.join("pid")))
                            },
                        } {
                            clean_up_path_sync(&path)?;
                        }
                        thread::sleep(Duration::from_secs(1));
                        continue
                    }
                    _ => return Err(e.at(path)),
                },
            }
        }
    }

    /// Return the contained Path.
    pub fn path(&self) -> &Path {
        self.0.as_path()
    }

    /// Unlocks this lock without blocking the thread.
    pub async fn drop_async(self) -> Result<(), Error> {
        self.clean_up().await?;
        forget(self);
        Ok(())
    }

    async fn clean_up(&self) -> Result<(), Error> {
        clean_up_path(&self.0).await
    }

    fn clean_up_sync(&self) -> Result<(), Error> {
        clean_up_path_sync(&self.0)
    }
}

impl Drop for DirLock {
    /// Unlocks this lock, blocking the current thread while doing so.
    ///
    /// # Panics
    ///
    /// Unlocking a directory lock involves I/O. If an error occurs, this method will panic.
    /// It is recommended to use [`drop_async`](Self::drop_async) instead, which returns the error.
    fn drop(&mut self) {
        self.clean_up_sync().expect("failed to clean up dir lock");
    }
}

async fn clean_up_path(path: &Path) -> Result<(), Error> {
    if let Err(e) = fs::remove_file(path.join("pid")).await {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e.at(path.join("pid")));
        }
    }
    if let Err(e) = fs::remove_dir(path).await {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e.at(path))
        }
    }
    Ok(())
}

fn clean_up_path_sync(path: &Path) -> Result<(), Error> {
    if let Err(e) = std::fs::remove_file(path.join("pid")) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e.at(path.join("pid")))
        }
    }
    if let Err(e) = std::fs::remove_dir(path) {
        if e.kind() != io::ErrorKind::NotFound {
            return Err(e.at(path))
        }
    }
    Ok(())
}

fn pid_exists(pid: Pid) -> bool {
    let mut system = sysinfo::System::default();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::default()) > 0
}
