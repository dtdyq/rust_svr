use std::any::{Any, TypeId};
use std::{env, fs};
use std::env::VarError;
use std::env::VarError::NotUnicode;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::DirEntry;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use config::{Config, File};
use dashmap::{DashMap, DashSet};
use serde::Deserialize;
use crate::cfg::configure::CfgRefreshEvent::{After, Before};
use crate::exe_parent;

#[derive(PartialEq,Debug)]
pub enum CfgRefreshEvent {
    Before,
    After,
}
#[derive(Clone)]
pub struct Configure {
    cfg_dir:PathBuf,
    cfg:Arc<DashMap<String,Config>>,
    typed_cfg:Arc<DashMap<TypeId,Arc<dyn Any + Send + Sync>>>,
    listeners:Arc<Mutex<Vec<(CfgRefreshEvent,Box<dyn Fn(&Configure) + Send>)>>>
}

impl Configure {
    pub fn from_dir(dir:PathBuf) -> Result<Self,CfgError>  {
        if !dir.exists() {
            return Err(CfgError::PathInvalid(format!("dir not exist:{:?}", dir)))
        }
        if !dir.is_dir() {
            return Err(CfgError::PathInvalid(format!("not a dir:{:?}", dir)))
        }

        let cfg_map = Self::load_config_map(&dir)?;
        Ok(Configure{cfg_dir:dir, cfg: Arc::new(cfg_map), typed_cfg: Arc::new(DashMap::new())
        ,listeners:Arc::new(Mutex::new(Vec::new()))})

    }

    fn load_config_map(dir: &PathBuf) -> Result<DashMap<String, Config>, CfgError> {
        let files = config_files_of_dir(&dir, &["toml".to_string(), "json".to_string()])?;

        let cfg_map = DashMap::new();
        for f in files {
            let key = f.file_name()
                .ok_or(CfgError::PathInvalid(format!("invalid file path:{:?}", f)))
                .and_then(|s| s.to_str().ok_or(CfgError::PathInvalid(format!("invalid os path:{:?}", f))))?
                .rsplitn(2, ".").last()
                .ok_or(CfgError::PathInvalid(format!("invalid file name:{:?}", f)))?;
            let cfg = path_to_config(&f)?;
            cfg_map.insert(key.to_string(), cfg);
        }
        Ok(cfg_map)
    }
    pub fn from_env() -> Result<Self,CfgError> {
        let pb = env_cfg_dir()?;
        Configure::from_dir(pb)
    }



    pub fn of<T>(&self,s:&str) -> Result<Arc<T>, CfgError> where T : for<'de> Deserialize<'de> + Send + Sync + 'static {
        let type_id = TypeId::of::<T>();
        if let Some(cached) = self.typed_cfg.get(&type_id) {
            if let Ok(arc_t) = cached.value().clone().downcast::<T>() {
                return Ok(arc_t);
            }
        }
        match self.cfg.get(s) {
            None=>Err(CfgError::CfgNotFound),
            Some(v)=> {
                let av  =v.value().clone();
                drop(v);
                match av.try_deserialize::<T>() {
                    Ok(d) => {
                        let db = Arc::new(d);
                        self.typed_cfg.insert(type_id, db.clone());
                        Ok(db)
                    },
                    Err(e)=>{
                        Err(crate::cfg::CfgError::Other(Box::from(e)))
                    }
                }
            }
        }
    }
    //
    // pub fn get<T>(s : &str) -> Option<T> where T: for<'de> Deserialize<'de> {
    //     let rg = CONFIG_INST.read().unwrap();
    //     match rg.get::<T>(s) {
    //         Ok(r) =>{
    //             Some(r)
    //         },
    //         Err(_)=>{
    //             None
    //         }
    //     }
    // }

    pub fn add_refresh_listen(&mut self,point:CfgRefreshEvent, f:impl Fn(&Configure)+Send + 'static) {
        self.listeners.lock().unwrap().push((point,Box::new(f)))
    }

    pub fn refresh(&mut self) ->Result<(),CfgError>{
        self.listeners.lock().unwrap().iter().filter(|t|t.0 == Before).for_each(|f|f.1(&self));
        let nm = Self::load_config_map(&self.cfg_dir)?;
        self.cfg.clear();
        for (k,v) in nm {
            self.cfg.insert(k,v);
        }
        self.typed_cfg.clear();
        self.listeners.lock().unwrap().iter().filter(|t|t.0 == After).for_each(|f|f.1(&self));
        Ok(())
    }
}

fn env_cfg_dir() -> Result<PathBuf,CfgError> {
    let pb = match env::var("CONFIG") {
        Ok(ecd) =>{
            PathBuf::from(ecd)
        }
        Err(e)=>{
            match e {
                VarError::NotPresent => {
                    match exe_parent() {
                        Some(p) => {
                            p.join("./config")
                        }
                        None => {
                            return Err(crate::cfg::CfgError::PathInvalid(format!("exe dir error")))
                        }
                    }
                }
                NotUnicode(nue) => {
                    return Err(crate::cfg::CfgError::Other(Box::from(NotUnicode(nue))))
                }
            }
        }
    };

    if !pb.exists() {
        return Err(crate::cfg::CfgError::PathInvalid(format!("config dir not found:{:?}", pb)))
    }
    if !pb.is_dir() {
        return Err(crate::cfg::CfgError::PathInvalid(format!("path not dir:{:?}", pb)))
    }
    Ok(pb)
}


/**
 * p:/etc/works/config ..etc
 * formats:["toml","json"] ..etc
 * ret: OK(["log.toml","db.json",..])
 **/
fn config_files_of_dir(p: &PathBuf, formats:&[String]) -> Result<Vec<PathBuf>,CfgError>  {
    let rd = fs::read_dir(p).map_err(|e| CfgError::Other(Box::from(format!("read dir error:{}", e))))?;
    let mut file_paths = Vec::new();
    fn retrieve_dir_entry(de:DirEntry,recv:&mut Vec<PathBuf>,formats:&[String]) {
        if de.path().is_dir() {
            retrieve_dir_entry(de,recv,formats);
        } else {
            let pb = de.path();
            if formats.iter().any(|f|pb.extension().and_then(|o|o.to_str()).unwrap_or("") == f) {
                recv.push(de.path());
            }
        }
    }
    for re in rd {
        match re {
            Ok(de) => {
                retrieve_dir_entry(de,&mut file_paths,&formats);
            }
            Err(e) => {
                return Err(crate::cfg::CfgError::Other(Box::try_from(e).unwrap()))
            }
        }
    }
    return Ok(file_paths)
}

fn path_to_config(pb: &PathBuf) -> Result<Config,CfgError> {
    let vs = match pb.to_str() {
        Some(v)=>v,
        None => {
            return Err(CfgError::PathInvalid(format!("{:?}",pb)))
        }
    };
    let mut config  = Config::builder();
    config = config.add_source(File::with_name(vs));
    config.build().map_err(|e| crate::cfg::CfgError::Other(Box::from(e)))
}
// =============== error define ===============

#[derive(Debug)]
pub enum CfgError {
    PathInvalid(String),
    CfgNotFound,
    DefaultConfigMissing,
    Other(Box<dyn Error>)
}

impl Display for CfgError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f,"cfg error:{:?}",self)
    }
}

impl Error for CfgError {
}

impl From<std::io::Error> for CfgError {
    fn from(error: std::io::Error) -> Self {
        CfgError::Other(Box::try_from(error).unwrap())
    }
}

impl From<config::ConfigError> for CfgError {
    fn from(value: config::ConfigError) -> Self {
        CfgError::Other(Box::try_from(format!("{}", value)).unwrap())
    }
}