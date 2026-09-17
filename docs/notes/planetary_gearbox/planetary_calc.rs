use waffle_types::gear_planetary::{generate_planetary, PlanetaryParams};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let p = PlanetaryParams {
        module: args[1].parse().unwrap(),
        pressure_angle_deg: 20.0,
        sun_teeth: args[2].parse().unwrap(),
        planet_teeth: args[3].parse().unwrap(),
        planet_count: args[4].parse().unwrap(),
        backlash: args[5].parse().unwrap(),
        center_x: 0.0, center_y: 0.0, auto_adjust: false,
    };
    match generate_planetary(&p) {
        Ok(r) => println!("{}", serde_json::to_string_pretty(&r).unwrap()),
        Err(e) => { eprintln!("ERROR: {e}"); std::process::exit(1); }
    }
}
