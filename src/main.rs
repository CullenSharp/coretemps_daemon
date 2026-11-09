use coretemps_daemon::collect_sensor_readings;
use serde_json::to_writer;
use std::eprintln;
use std::io::Error;
use std::io::Write;
use std::matches;
use std::os::unix::net::{UnixListener, UnixStream};
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs, io, path::Path};

fn main() {
    if let Err(err) = run() {
        eprintln!("coretemps_daemon: {err}");
        std::process::exit(1);
    }
}

// send data until failure or client disconnect
fn handle_client(mut stream: UnixStream) -> io::Result<()> {
    loop {
        let sensor_readings = collect_sensor_readings()?;

        to_writer(&stream, &sensor_readings)?;
        stream.write_all(b"\n")?;

        // sleep 100ms
        sleep(Duration::from_millis(100));
    }
}

fn is_disconnect(error: &Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}

fn run() -> io::Result<()> {
    // TODO: add a process lock
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

    // listener handles the error
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("coretemps_daemon: connection failure: {error}");
                continue;
            }
        };

        if let Err(error) = handle_client(stream)
            && !is_disconnect(&error)
        {
            eprintln!("coretemps_daemon: client error: {error}");
            return Err(error);
        }
    }

    Ok(())
}
