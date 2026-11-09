use coretemps_daemon::collect_sensor_readings;
use serde_json::to_writer;
use std::io::Write;
use std::{env, fs, io, path::Path};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;
use std::thread::sleep;

fn main() {
    if let Err(err) = run() {
        eprintln!("coretemps_daemon: {err}");
        std::process::exit(1);
    }
}

// send data until failure or client disconnect
fn handle_client(mut stream: UnixStream) -> io::Result<()> {
    loop  {
        let sensor_readings = collect_sensor_readings()?;

        to_writer(&stream, &sensor_readings)?;
        stream.write_all(b"\n")?;

        // sleep 100ms
        sleep(Duration::from_millis(100));
    }
}

fn run() -> io::Result<()> {
    let runtime_dir = Path::new(&env::var("XDG_RUNTIME_DIR").unwrap()).to_path_buf();
    let socket = runtime_dir.join("coretempd/coretempd.sock");
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)?;
    }

    // remove a stale socket
    if socket.exists() {
        fs::remove_file(&socket)?;
    }

    let listener = UnixListener::bind(&socket)?;

    for stream in listener.incoming() {
        let stream = stream?;
        handle_client(stream)?;
    }

    Ok(())
}
