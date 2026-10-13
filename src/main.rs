#![windows_subsystem = "windows"]
#![allow(unsafe_op_in_unsafe_fn)]

mod picker;
mod ui;

use std::{path::PathBuf, sync::atomic::AtomicBool};
use stitch::media::{self, Runtime, Source};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let runtime = std::env::current_exe()
        .map_err(|e| e.to_string())
        .and_then(|exe| Runtime::beside(&exe));
    if args
        .first()
        .is_some_and(|s| s == "--join" || s == "--inspect")
    {
        let result = cli(&args, runtime);
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    ui::run(args, runtime);
}

fn cli(args: &[String], runtime: media::Result<Runtime>) -> media::Result<()> {
    let runtime = runtime?;
    let canceled = AtomicBool::new(false);
    if args[0] == "--inspect" {
        let path = args.get(1).ok_or("Usage: Stitch.exe --inspect <video>")?;
        println!(
            "{}",
            serde_json::to_string_pretty(&media::inspect(
                &runtime,
                &PathBuf::from(path),
                &canceled
            )?)
            .unwrap()
        );
        return Ok(());
    }
    if args.len() < 3 {
        return Err("Usage: Stitch.exe --join <output.mp4> <videos...>".into());
    }
    let mut sources = Vec::new();
    for input in &args[2..] {
        let path = PathBuf::from(input);
        let info = media::inspect(&runtime, &path, &canceled)?;
        sources.push(Source { path, info });
    }
    let mode = media::export(
        &runtime,
        &sources,
        &PathBuf::from(&args[1]),
        media::ExportOptions::default(),
        &canceled,
        |_| {},
    )?;
    println!("{}", serde_json::json!({"mode":mode,"output":args[1]}));
    Ok(())
}
