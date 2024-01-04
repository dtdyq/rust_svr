use std::env::current_exe;
use std::fmt::Error;
use std::path::PathBuf;

#[cfg(feature = "cfg")]
pub mod cfg;

#[cfg(feature = "trace")]
pub mod trace;


#[cfg(feature = "lock")]
pub mod lock;


pub(crate) fn exe_parent() -> Option<PathBuf> {
    if let Ok(v) = current_exe() {
        match v.parent() {
            None => None,
            Some(p) => {
                match p.canonicalize() {
                    Ok(pa) =>Some(pa),
                    Err(e)=>None
                }
            }
        }
    } else {
        None
    }
}
