mod sensor_readings;

use std::io::{self, Error, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs, matches};

use serde_json::to_writer;

fn main() {
    if let Err(err) = run() {
        eprintln!("coretemps_daemon: {err}");
        std::process::exit(1);
    }
}

// send data until failure or client disconnect
fn handle_client(mut stream: UnixStream) -> io::Result<()> {
    println!("Handling client...");
    loop {
        let sensor_readings = sensor_readings::collect()?;

        to_writer(&stream, &sensor_readings)?;
        stream.write_all(b"\n")?;

        // sleep 100ms
        sleep(Duration::from_millis(1000));
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

        println!("Connected: {stream:?}");

        if let Err(error) = handle_client(stream)
            && !is_disconnect(&error)
        {
            eprintln!("coretemps_daemon: client error: {error}");
            return Err(error);
        }
    }

    Ok(())
}
