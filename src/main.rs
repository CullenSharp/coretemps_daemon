use coretemps_daemon::collect_sensor_readings;
use std::io;

fn main() {
    if let Err(err) = run() {
        eprintln!("coretemps_daemon: {err}");
        std::process::exit(1);
    }
}

fn run() -> io::Result<()> {
    for sensor_reading in collect_sensor_readings()? {
        let serialized = serde_json::to_string(&sensor_reading).unwrap();
        println!("{}", serialized );
    }
    Ok(())
}
