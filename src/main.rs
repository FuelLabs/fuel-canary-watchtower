use std::env;

pub static WATCHTOWER_CONFIG_FILE: &str = "watchtower_config.json";
pub static LOGGING_CONFIG_FILE: &str = "logging_config.yaml";

#[tokio::main]
async fn main() {
    // setup logging
    log4rs::init_file(LOGGING_CONFIG_FILE, Default::default()).unwrap();

    // determine the config file to use
    let mut config_file = WATCHTOWER_CONFIG_FILE;
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let arg = &args[1];
        if arg.ends_with(".json") {
            config_file = arg;
            log::info!("Using config file: {}", config_file);
        } else {
            log::info!("Invalid config file specified: {}", arg);
            log::info!("Using default config file: {}", config_file);
        }
    } else {
        log::info!("Using default config file: {}", config_file);
    }

    // get the watchtower config
    let config_result = fuel_canary_watchtower::load_config(
        config_file,
    );
    if config_result.is_err() {
        log::error!("Failed to load config: {}", config_result.err().unwrap());
    } else {
        let config = config_result.unwrap();

        // start the watchtower
        let run_result = fuel_canary_watchtower::run(&config).await;
        if run_result.is_err() {
            log::error!("{}", run_result.err().unwrap());
        }
    }
}
