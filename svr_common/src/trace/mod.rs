use std::{env, fs};
use std::any::Any;
use std::io::Write;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use chrono::Local;
use lazy_static::lazy_static;
use tracing::{info, Level};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_appender::rolling;
use tracing_subscriber::{fmt, Layer, Registry};
use tracing_subscriber::fmt::format::{Format, Full, Writer};
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::fmt::writer::{MakeWriterExt, Tee};
use tracing_subscriber::layer::SubscriberExt;

use crate::exe_parent;
use tracing_subscriber::fmt::MakeWriter;
use crate::cfg::{Configure, LogConfig, SvrCfg};

struct LocalTraceTime;

impl FormatTime for LocalTraceTime {
    fn format_time(&self, w: &mut Writer<'_>) -> core::fmt::Result {
        let now = Local::now();
        write!(w, "{}", now.format("%Y-%m-%d %H:%M:%S%.3f"))
    }
}
pub fn init(configure: &Configure) -> Vec<WorkerGuard> {
    let cfg = &configure.of::<SvrCfg>("config").unwrap().log;
    let mut wgs = Vec::new();
    if !cfg.to_file && !cfg.to_stdout {
        return wgs;
    }
    let log_dir = setup(&cfg);

    let level_filter = tracing_subscriber::filter::LevelFilter::from_level(Level::from_str(&cfg.level).unwrap_or(Level::INFO));
    let event_layer = fmt::Layer::new()
        .event_format(
            event_formatter(&cfg)
        )
        .with_span_events(fmt::format::FmtSpan::FULL)
        .with_ansi(false);


    match (cfg.to_stdout, cfg.to_file) {
        (true, true) => {
            let file_appender = rolling::daily(log_dir.as_path(), &cfg.file_name);
            let (file, gf) = tracing_appender::non_blocking(file_appender);
            let (stdout, go) = tracing_appender::non_blocking(std::io::stdout());
            wgs.push(go);
            wgs.push(gf);
            println!("write both");
            let r  =Registry::default().with(event_layer.with_writer(stdout.and(file))).with(level_filter);

            tracing::subscriber::set_global_default(r).expect("init tracing error");
        },
        (true,false) =>{
            let (stdout, go) = tracing_appender::non_blocking(std::io::stdout());
            wgs.push(go);
            println!("write std");

            let r  =Registry::default().with(event_layer.with_writer(stdout)).with(level_filter);

            tracing::subscriber::set_global_default(r).expect("init tracing error");
        },
        (false,true) =>{
            let file_appender = rolling::daily(log_dir.as_path(), &cfg.file_name);
            let (file, gf) = tracing_appender::non_blocking(file_appender);
            wgs.push(gf);
            println!("write f");
            let r  =Registry::default().with(event_layer.with_writer(file)).with(level_filter);

            tracing::subscriber::set_global_default(r).expect("init tracing error");
        },
        (_) =>{return wgs}
    };
    info!("trace init end");
    wgs
}

fn event_formatter(cfg: &LogConfig) -> Format<Full, LocalTraceTime> {
    let event_formatter = tracing_subscriber::fmt::format()
        .with_thread_ids(cfg.with_thread_id) // include the thread ID of the current thread
        .with_thread_names(cfg.with_thread_name)
        .with_timer(LocalTraceTime {})
        .with_line_number(cfg.with_line_number);
    event_formatter
}

fn setup(cfg: &LogConfig) -> PathBuf {
    let log_var = PathBuf::from(&cfg.dir);
    let home_dir = exe_parent().expect("exe dir invalid");
    let log_dir = if log_var.is_absolute() { log_var } else { home_dir.join(log_var) };
    if !log_dir.exists() {
        fs::create_dir_all(log_dir.clone()).expect("create log dir error")
    }
    log_dir
}