use std::{env, fs};
use std::any::{Any, TypeId};
use std::env::VarError;
use std::env::VarError::NotUnicode;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use config::{Config, File};
use dashmap::DashMap;
use serde::Deserialize;
use crate::cfg::CfgError::{CfgNotFound, Other, PathInvalid};
use crate::exe_parent;

mod models;
mod configure;

pub use models::SvrCfg;
pub use models::CsEndpointConfig;
pub use models::DbConfig;
pub use models::LogConfig;
pub use models::MysqlConfig;

pub use configure::Configure;
pub use configure::CfgError;


#[cfg(test)]
mod cfg_test {
    use std::env::current_dir;
    use std::future::join;
    use std::path::PathBuf;
    use std::thread;
    use std::thread::sleep;
    use std::time::{Duration, SystemTime};
    use crate::cfg::{Configure};
    use crate::cfg::configure::CfgRefreshEvent;
    use crate::cfg::models::SvrCfg;
    use crate::exe_parent;

    #[test]
    fn test_configure() {
        println!("exe dir:{:?} current dir:{:?}",exe_parent(),current_dir());
        println!("endwith:{:?}",&["toml".to_string(),"json".to_string()].iter().any(|f|"../config\\config.toml".ends_with(f)));
        let p = PathBuf::from("../config");
        println!("path:{:?}",p.exists());
        let mut cfg = Configure::from_dir("../config".into()).unwrap();
        let svr = cfg.of::<SvrCfg>("config").unwrap();
        println!("{:?}",svr);
        cfg.add_refresh_listen(CfgRefreshEvent::Before,|c|{
            println!("refresh happen!")
        });
        let mut cfg_p = cfg.clone();
        let f = thread::spawn(move ||{
            let svr_t = cfg_p.of::<SvrCfg>("config").unwrap();
            println!("from thread{:?}",svr_t);
            cfg_p.refresh().unwrap();
            cfg_p.add_refresh_listen(CfgRefreshEvent::After,|c|{
                println!("refresh again")
            })
        });
        f.join().expect("TODO: panic message");
        cfg.refresh().unwrap()
    }

    #[test]
    fn test_concurrent() {
        let c = Configure::from_dir("../config".into()).unwrap();
        let mut cp1 = c.clone();
        let mut cp2 = c.clone();
        let mut cp3 = c.clone();
        let mut cp4 = c.clone();
        let j1 = thread::spawn(move ||{
            let mut cc = 0;
            loop {
                cc += 1;
                sleep(Duration::from_millis(10));
                cp1.refresh().expect("TODO: panic message");
                if cc == 9 {
                    cp1.add_refresh_listen(CfgRefreshEvent::Before,|c|{
                        println!("add before from j1")
                    });
                }
                if cc >= 10 {
                    break
                }
            }
        });
        let j2 = thread::spawn(move ||{
            let mut cc = 0;
            loop {
                cc += 1;
                sleep(Duration::from_millis(10));
                cp2.refresh().expect("TODO: panic message");
                if cc >= 10 {
                    break
                }
            }
        });
        let r3 = thread::spawn(move || {
            let mut cc = 0;
            loop {
                let cff = cp3.of::<SvrCfg>("config").unwrap();
                println!("read config r3:{}",cff.svr_id);
                sleep(Duration::from_millis(10));
                cc += 1;
                if cc >= 10 {
                    break
                }
            }
        });
        let r4 = thread::spawn(move || {
            let mut cc = 0;
            loop {
                let cff = cp4.of::<SvrCfg>("config").unwrap();
                println!("read config r4:{}",cff.svr_id);
                sleep(Duration::from_millis(10));

                if cc == 9 {
                    cp4.add_refresh_listen(CfgRefreshEvent::After,|c|{
                        println!("add after from r4")
                    });
                }
                cc += 1;
                if cc >= 10 {
                    break
                }
            }
        });
        j1.join().unwrap();
        j2.join().unwrap();
        r3.join().unwrap();
        r4.join().unwrap();
    }
    #[test]
    fn test_performance() {
        let mut c = Configure::from_dir("../config".into()).unwrap();
        let cur = SystemTime::now();
        for i in 0..100000 {
            let cff = c.of::<SvrCfg>("config").unwrap();
        }
        println!("time cost:{}",cur.elapsed().unwrap().as_millis());
        let cur1 = SystemTime::now();
        for i in 0..1000 {
            c.refresh().unwrap()
        }
        println!("refresh time cost:{}",cur1.elapsed().unwrap().as_millis())
    }
}











