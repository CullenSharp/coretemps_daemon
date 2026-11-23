use std::{fmt, fs, io};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/* consts */

const HWMON_PATH: &str = "/sys/class/hwmon/";

/* structs */

#[derive(Serialize, Deserialize, Debug)]
#[allow(dead_code)]
pub struct SensorReading {
    nr: u32,                    // channel number
    label: Option<String>,      // coreN
    critical_temp: Option<f64>, // deg C
    current_temp: Option<f64>,  // deg C
    max_temp: Option<f64>,      // deg C
}

impl fmt::Display for SensorReading {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let label = self.label.as_deref().unwrap_or("N/A");
        write!(f, "{label:<12}: ")?;

        if let Some(temp) = self.current_temp {
            write!(f, "{temp:.1}°C ")?;
        }

        if let Some(temp) = self.max_temp {
            write!(f, "max: {temp:.1}°C ")?;
        }

        if let Some(temp) = self.critical_temp {
            write!(f, " crit: {temp:.1}°C")?;
        };

        Ok(())
    }
}

// correlates raw data from sysfs
#[derive(Debug)]
struct TempSensor {
    nr: u32,
    label: Option<String>,
    critical_temp: Option<String>,
    current_temp: Option<String>,
    max_temp: Option<String>,
}

// conversion is fallible, see parse_celsius
impl TryFrom<TempSensor> for SensorReading {
    type Error = io::Error;

    fn try_from(sensor: TempSensor) -> Result<Self, Self::Error> {
        Ok(SensorReading {
            nr: sensor.nr,
            label: sensor.label,
            critical_temp: parse_celsius(sensor.critical_temp.as_deref())?,
            current_temp: parse_celsius(sensor.current_temp.as_deref())?,
            max_temp: parse_celsius(sensor.max_temp.as_deref())?,
        })
    }
}

impl TempSensor {
    fn new(nr: u32) -> TempSensor {
        TempSensor {
            nr,
            label: None,
            critical_temp: None,
            current_temp: None,
            max_temp: None,
        }
    }
}

/* private procedures */

/// Convert from raw sysfs string in millidegree to degree celsius
fn parse_celsius(raw: Option<&str>) -> io::Result<Option<f64>> {
    // if no string is provided, return None
    let Some(raw) = raw else {
        return Ok(None);
    };

    // parse, wrap error
    let millidegrees = raw.parse::<f64>().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid temperature reading {raw:?}: {error}"),
        )
    })?; // Propogate the error

    Ok(Some(millidegrees / 1000.0))
}

fn resolve_coretemp_path(hwmon_path: &Path) -> io::Result<PathBuf> {
    for entry in fs::read_dir(hwmon_path)? {
        let dir = entry?.path();

        // /sys/class/hwmon/hwmon9 -> /sys/class/hwmon/hwmon9/name
        let Ok(name) = fs::read_to_string(dir.join("name")) else {
            // Skip unreadable
            continue;
        };

        if name.trim() == "coretemp" {
            return Ok(dir);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("coretemp not found in {}", hwmon_path.display()),
    ))
}

fn to_sensor_readings(sensors: Vec<TempSensor>) -> io::Result<Vec<SensorReading>> {
    sensors.into_iter().map(SensorReading::try_from).collect()
}

fn parse_hwmon_entry(file_name: &str) -> Option<(u32, &str)> {
    // temp10_input -> 10_input
    let node = file_name.strip_prefix("temp")?;
    let (node, subfeature) = node.split_once('_')?;
    Some((node.parse::<u32>().ok()?, subfeature))
}

fn collect_temp_sensors(hwmon_path: &Path) -> io::Result<Vec<TempSensor>> {
    let mut temp_sensors: Vec<TempSensor> = Vec::new();
    let coretemp_hwmon_path = resolve_coretemp_path(hwmon_path)?;
    for entry in fs::read_dir(coretemp_hwmon_path)? {
        let entry = entry?;
        let path = entry.path();

        // entry.path() is the full path (/sys/class/hwmon/hwmon8/temp1_input),
        // so filter and split on the file name only
        let file_name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => {
                continue;
            }
        };

        if !file_name.starts_with("temp") {
            continue;
        }

        let Some((nr, sub_feature)) = parse_hwmon_entry(file_name) else {
            continue;
        };

        let temp_sensor = temp_sensors
            .iter_mut()
            .find(|temp_sensor| temp_sensor.nr == nr);

        let temp_sensor = match temp_sensor {
            Some(temp_sensor) => temp_sensor,
            None => {
                temp_sensors.push(TempSensor::new(nr));
                temp_sensors.last_mut().unwrap()
            }
        };

        let value = fs::read_to_string(&path).map_err(|error| {
            io::Error::new(error.kind(), format!("{}: {error}", path.display()))
        })?;

        let value = value.trim().to_string();

        match sub_feature {
            "input" => temp_sensor.current_temp = Some(value),
            "crit" => temp_sensor.critical_temp = Some(value),
            "max" => temp_sensor.max_temp = Some(value),
            "label" => temp_sensor.label = Some(value),
            _ => {}
        }
    }

    temp_sensors.sort_by_key(|temp_sensor| temp_sensor.nr);

    Ok(temp_sensors)
}

/* public procedures */

/// Collects temperature sensor readings from the system and returns a vector of SensorReading structs.
///
/// # Errors
///
/// - Fails to find coretemp on hwmon path
/// - Fails to parse temperature strings
pub fn collect() -> io::Result<Vec<SensorReading>> {
    let hwmon_path = Path::new(HWMON_PATH);
    let temp_sensors = collect_temp_sensors(hwmon_path)?;
    let sensor_readings = to_sensor_readings(temp_sensors)?;
    Ok(sensor_readings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // set up a faked hwmon dir
    fn write_chip(root: &Path, dir: &str, files: &[(&str, &str)]) -> PathBuf {
        let chip = root.join(dir);
        fs::create_dir_all(&chip).unwrap();
        for (name, value) in files {
            fs::write(chip.join(name), value).unwrap();
        }
        chip
    }

    #[test]
    fn resolve_coretemp_path_coretemp_not_found() {
        let result = resolve_coretemp_path(TempDir::new().unwrap().path());
        assert!(result.is_err(), "Expected an error. Got {result:?}");
    }

    #[test]
    fn resolve_coretemp_path_coretemp_found() {
        let root = TempDir::new().unwrap();
        write_chip(root.path(), "hwmon0", &[("name", "coretemp")]);
        let result = resolve_coretemp_path(root.path());
        assert!(result.is_ok(), "Expected ok. Got {result:?}");
    }

    #[test]
    fn to_sensor_readings_fails_invalid_data() {
        let root = TempDir::new().unwrap();
        write_chip(
            root.path(),
            "hwmon0",
            &[("name", "coretemp"), ("temp10_input", "foobar")],
        );
        let result = to_sensor_readings(collect_temp_sensors(root.path()).unwrap());
        assert!(result.is_err(), "Expected ok. Got {result:?}");
    }

    #[test]
    fn to_sensor_readings_passes_with_sparse_features() {
        let root = TempDir::new().unwrap();
        write_chip(
            root.path(),
            "hwmon0",
            &[("name", "coretemp"), ("temp10_input", "32000")],
        );
        let result = to_sensor_readings(collect_temp_sensors(root.path()).unwrap());
        assert!(result.is_ok(), "Expected ok. Got {result:?}");
    }

    #[test]
    fn to_sensor_readings_passes_with_parsed_data() {
        let root = TempDir::new().unwrap();
        write_chip(
            root.path(),
            "hwmon0",
            &[("name", "coretemp"), ("temp10_input", "32000")],
        );
        let result = to_sensor_readings(collect_temp_sensors(root.path()).unwrap()).unwrap();
        assert_eq!(result[0].current_temp, Some(32.0));
    }

    #[test]
    fn parse_hwmon_entry_returns_none() {
        // input, expected
        let cases = [("", None), ("temperature1_x", None), ("name", None)];

        for (input, expected) in cases {
            assert_eq!(parse_hwmon_entry(input), expected);
        }
    }

    #[test]
    fn parse_hwmon_entry_returns_tuple() {
        // input, expected
        let cases = [
            ("temp1_input", Some((1u32, "input"))),
            ("temp10_crit_hyst", Some((10u32, "crit_hyst"))),
        ];

        for (input, expected) in cases {
            assert_eq!(parse_hwmon_entry(input), expected);
        }
    }
}
