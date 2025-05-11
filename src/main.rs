

use std::collections::HashMap;
use nu_json_api_rs::evaluate_command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        if args.len() > pos + 1 {
            let cmd = &args[pos + 1];
            let mut env_vars = HashMap::new();
            for (k, v) in std::env::vars() {
                env_vars.insert(k, v);
            }

            #[cfg(target_os = "windows")]
            {
                env_vars.insert("PWD".to_string(), std::env::current_dir().unwrap().to_str().unwrap().to_string());
            }

            let result = evaluate_command(cmd, env_vars);
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        } else {
            eprintln!("Error: '-c' flag without command.");
            std::process::exit(1);
        }
    } else {
        eprintln!(
            "Usage: {} -c 'command'",
            args.get(0).unwrap_or(&"nushell_runner".to_string())
        );
        std::process::exit(1);
    }
}
