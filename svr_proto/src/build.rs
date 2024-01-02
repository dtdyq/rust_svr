// main

use std::{env, fs};
use std::collections::HashMap;
use std::fs::{create_dir, File};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use prost_build::{Config, protoc_from_env, protoc_include_from_env};
use prost_types::FileDescriptorSet;
use prost::Message;
use std::io::Write;
use std::sync::OnceLock;
use dashmap::{DashMap, DashSet};

static MODULE_OUT_MAP:OnceLock<DashMap<&str,&str>> = OnceLock::new();
fn gen_mod_rs(s:&str) {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let proto_dir =PathBuf::from(manifest_dir).join(MODULE_OUT_MAP.get().unwrap().get(s).unwrap().value());

    let mod_file_path = proto_dir.join("mod.rs");
    let mut mod_file = File::create(&mod_file_path).unwrap();

    for entry in fs::read_dir(proto_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let module_name = path.file_stem().and_then(|s| s.to_str()).unwrap();
            if module_name != "mod" {
                writeln!(mod_file, "pub mod {};", module_name).unwrap();
            }
        }
    }
}

fn cs_protos() -> Vec<String> {
    // 指定 `.proto` 文件所在的目录
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let pl = PathBuf::from(manifest_dir.clone()).to_str().unwrap().len();
    let proto_files_dir = PathBuf::from(manifest_dir).join("protos").join("cs");
    fs::read_dir(proto_files_dir)
        .expect("Failed to read proto files directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.is_file() && path.extension()? == "proto" {
                let p = path.to_str().unwrap().clone();
                Some(p.replace("\\","/")[pl+1..].to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
}

fn generate_proto(module : &str) {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("asda{}",manifest_dir.clone());
    let proto_dir =PathBuf::from(manifest_dir).join(format!("src/{}",module.clone()));
    if Path::exists(proto_dir.as_path()) {
        fs::remove_dir_all(proto_dir.as_path()).unwrap();
    }
    fs::create_dir_all(proto_dir.as_path()).unwrap();

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let mut config = Config::new();
    config.message_attribute(".","#[derive(svr_macro::IntoCsResponse)]");
    config.out_dir(PathBuf::from(manifest_dir).join(MODULE_OUT_MAP.get().unwrap().get(module).unwrap().value()));


    let includes = &[format!("protos/{}",module.clone())];

    config.compile_protos(&cs_protos(), includes).unwrap();

    gen_mod_rs(module);
    if module == "cs" {
        let fd_set = compile_fd_set(&cs_protos(), includes).unwrap();
        println!("{:?}", fd_set.file.len())
    }
}
fn main() {
    MODULE_OUT_MAP.get_or_init(||{
        let mut m = DashMap::new();
        m.insert("cs","../svr_endpoint/src/cs/proto");
        m
    });
    generate_proto("cs");
}
pub fn compile_fd_set(
    protos: &[impl AsRef<Path>],
    includes: &[impl AsRef<Path>],
) -> Result<FileDescriptorSet,std::io::Error> {
    // TODO: This should probably emit 'rerun-if-changed=PATH' directives for cargo, however
    // according to [1] if any are output then those paths replace the default crate root,
    // which is undesirable. Figure out how to do it in an additive way; perhaps gcc-rs has
    // this figured out.
    // [1]: http://doc.crates.io/build-script.html#outputs-of-the-build-script

    let file_descriptor_set_path = tempfile::tempdir().unwrap().into_path().join("fd_set");
    let protoc = protoc_from_env();

    let mut cmd = Command::new(protoc.clone());
    cmd.arg("--include_imports")
        .arg("--include_source_info")
        .arg("-o")
        .arg(&file_descriptor_set_path);


    for include in includes {
        if include.as_ref().exists() {
            cmd.arg("-I").arg(include.as_ref());
        } else {
            println!(
                        "ignoring {} since it does not exist.",
                        include.as_ref().display()
                    )
        }
    }
    if let Some(protoc_include) = protoc_include_from_env() {
        cmd.arg("-I").arg(protoc_include);
    }

    for proto in protos {
        cmd.arg(proto.as_ref());
    }

    let output = cmd.output().map_err(|error| {
        Error::new(
            error.kind(),
            format!("failed to invoke protoc (hint: https://docs.rs/prost-build/#sourcing-protoc): (path: {:?}): {}", &protoc, error),
        )
    })?;

    if !output.status.success() {
        return Err(Error::new(
            ErrorKind::Other,
            format!("protoc failed: {}", String::from_utf8_lossy(&output.stderr)),
        ));
    }
    let buf = fs::read(&file_descriptor_set_path).map_err(|e| {
        Error::new(
            e.kind(),
            format!(
                "unable to open file_descriptor_set_path: {:?}, OS: {}",
                &file_descriptor_set_path, e
            ),
        )
    })?;
    let file_descriptor_set = FileDescriptorSet::decode(&*buf).map_err(|error| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("invalid FileDescriptorSet: {}", error),
        )
    })?;
    Ok(file_descriptor_set)
}