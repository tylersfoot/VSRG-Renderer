use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about = "VSRG Renderer")]
struct CliArgs {
    /// Directory containing the map (.qua) file
    map_dir: PathBuf,

    /// Start in fullscreen
    #[arg(long)]
    fullscreen: bool,

    /// Playback rate
    #[arg(long, default_value_t = 1.0)]
    rate: f64,

    /// Initial audio volume
    #[arg(long, default_value_t = 0.03)]
    volume: f64,

    /// Mirror notes horizontally
    #[arg(long)]
    mirror: bool,

    /// Ignore scroll velocities
    #[arg(long)]
    no_sv: bool,

    /// Ignore scroll speed factors
    #[arg(long)]
    no_ssf: bool,
}

#[test]
fn defaults_are_correct() {
    let args = CliArgs::parse_from(["test", "some/path"]);
    assert_eq!(args.rate, 1.0);
    assert_eq!(args.volume, 0.03);
    assert!(!args.fullscreen);
    assert!(!args.mirror);
    assert!(!args.no_sv);
    assert!(!args.no_ssf);
}