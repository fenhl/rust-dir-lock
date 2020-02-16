#![deny(rust_2018_idioms, unused, unused_import_braces, unused_qualifications, warnings)]

use {
    std::{
        fs::File as SyncFile,
        io::{
            self,
            Read as _,
            Write as _
        },
        mem::forget,
        num::ParseIntError,
        path::{
            Path,
            PathBuf
        },
        thread,
        time::Duration
    },
    async_std::{
        fs::{
            self,
            File
        },
        io::prelude::*,
        task::{
            block_on,
            sleep
        }
    },
    derive_more::From,
    heim::process::pid_exists
};

#[must_use = "must call the drop_async method to unlock"]
pub struct DirLock<'a>(&'a Path);

#[derive(Debug, From)]
pub enum Error {
    HeimProcess(heim::process::ProcessError),
    #[from(ignore)]
    Io(io::Error, Option<PathBuf>),
    ParseInt(ParseIntError)
}

pub trait IoResultExt {
    type T;

    fn at(self, path: impl AsRef<Path>) -> Self::T;
}

impl IoResultExt for io::Error {
    type T = Error;

    fn at(self, path: impl AsRef<Path>) -> Error {
        Error::Io(self, Some(path.as_ref().to_owned()))
    }
}

impl<T, E: IoResultExt> IoResultExt for Result<T, E> {
    type T = Result<T, E::T>;

    fn at(self, path: impl AsRef<Path>) -> Result<T, E::T> {
        self.map_err(|e| e.at(path))
    }
}

impl DirLock<'_> {
    pub async fn new(path: &impl AsRef<Path>) -> Result<DirLock<'_>, Error> {
        let path = path.as_ref();
        loop {
            match fs::create_dir(path).await { // see https://github.com/rust-lang/rustup.rs/issues/988
                Ok(()) => {
                    let pidfile = path.join("pid");
                    writeln!(SyncFile::create(&pidfile).at(&pidfile)?, "{}", std::process::id()).at(pidfile)?; //TODO replace SyncFile with File once format_args! is Sync
                    return Ok(DirLock(path));
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        if match File::open(path.join("pid")).await {
                            Ok(mut f) => {
                                let mut buf = String::default();
                                f.read_to_string(&mut buf).await.at(path.join("pid"))?;
                                !buf.is_empty() // assume pidfile is still being written if empty //TODO check timestamp
                                && !pid_exists(buf.trim().parse()?).await?
                            }
                            Err(e) => if e.kind() == io::ErrorKind::NotFound {
                                false
                            } else {
                                return Err(e.at(path.join("pid")));
                            }
                        } {
                            DirLock(path).clean_up().await?;
                        }
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    _ => { return Err(e.at(path)); }
                }
            }
        }
    }

    /// Blocks the current thread until the lock can be established.
    pub fn new_sync(path: &impl AsRef<Path>) -> Result<DirLock<'_>, Error> {
        let path = path.as_ref();
        loop {
            match std::fs::create_dir(path) { // see https://github.com/rust-lang/rustup.rs/issues/988
                Ok(()) => {
                    let pidfile = path.join("pid");
                    writeln!(SyncFile::create(&pidfile).at(&pidfile)?, "{}", std::process::id()).at(pidfile)?;
                    return Ok(DirLock(path));
                }
                Err(e) => match e.kind() {
                    io::ErrorKind::AlreadyExists => {
                        if match SyncFile::open(path.join("pid")) {
                            Ok(mut f) => {
                                let mut buf = String::default();
                                f.read_to_string(&mut buf).at(path.join("pid"))?;
                                !buf.is_empty() // assume pidfile is still being written if empty //TODO check timestamp
                                && !block_on(pid_exists(buf.trim().parse()?))?
                            }
                            Err(e) => if e.kind() == io::ErrorKind::NotFound {
                                false
                            } else {
                                return Err(e.at(path.join("pid")));
                            }
                        } {
                            DirLock(path).clean_up_sync()?;
                        }
                        thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    _ => { return Err(e.at(path)); }
                }
            }
        }
    }

    pub async fn drop_async(self) -> Result<(), Error> {
        self.clean_up().await?;
        forget(self);
        Ok(())
    }

    async fn clean_up(&self) -> Result<(), Error> {
        if let Err(e) = fs::remove_file(self.0.join("pid")).await {
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e.at(self.0.join("pid")));
            }
        }
        if let Err(e) = fs::remove_dir(self.0).await {
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e.at(self.0));
            }
        }
        Ok(())
    }

    fn clean_up_sync(&self) -> Result<(), Error> {
        if let Err(e) = std::fs::remove_file(self.0.join("pid")) {
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e.at(self.0.join("pid")));
            }
        }
        if let Err(e) = std::fs::remove_dir(self.0) {
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e.at(self.0));
            }
        }
        Ok(())
    }
}

impl Drop for DirLock<'_> {
    fn drop(&mut self) {
        self.clean_up_sync().expect("failed to clean up dir lock");
    }
}
