use nu_engine::get_eval_block;
use nu_parser::parse;
use nu_protocol::engine::{EngineState, Stack, StateWorkingSet};
use nu_protocol::{PipelineData, ShellError, Span, Value};
use nu_std::load_standard_library;
use nu_cmd_extra; // Add extra command context
use std::env;


fn get_engine_state() -> EngineState {
    let engine_state = nu_cmd_lang::create_default_context();
    let engine_state = nu_cmd_plugin::add_plugin_command_context(engine_state);
    let engine_state = nu_command::add_shell_command_context(engine_state);
    let engine_state = nu_cmd_extra::add_extra_command_context(engine_state);
    let engine_state = nu_cli::add_cli_context(engine_state);
    let engine_state = nu_explore::add_explore_context(engine_state);
    let mut engine_state = nu_explore::add_explore_context(engine_state);

       // Custom additions
       let delta = {
        let mut working_set = nu_protocol::engine::StateWorkingSet::new(&engine_state);
        working_set.add_decl(Box::new(nu_cli::NuHighlight));
        working_set.add_decl(Box::new(nu_cli::Print));
        working_set.render()
    };

    if let Err(err) = engine_state.merge_delta(delta) {
        report_shell_error(&engine_state, &err);
    }

    engine_state
}



fn main() {
    // Collect command-line arguments and current directory
    let args: Vec<String> = env::args().collect();
    let project_path = env::current_dir().unwrap_or_else(|e| {
        eprintln!("Error: Failed to get current directory: {}", e);
        std::process::exit(1);
    });

    // Initialize the engine state with standard Nushell command contexts
    let mut engine_state = get_engine_state();
    
    // Ingest all host environment variables into the Nushell engine
    for (key, val) in env::vars() {
        engine_state.add_env_var(
            key.clone(),
            Value::string(val.clone(), Span::unknown()),
        );
    }
    // Ensure NU_VERSION is explicit (overrides host value if present)
    engine_state.add_env_var(
        "NU_VERSION".into(),
        Value::string(env!("CARGO_PKG_VERSION"), Span::unknown()),
    );

    // Load the standard library and configure engine flags
    if let Err(err) = load_standard_library(&mut engine_state) {
        eprintln!("Error: Failed to load standard library: {}", err);
        std::process::exit(1);
    }
    engine_state.is_interactive = false;
    engine_state.is_login = false;
    engine_state.history_enabled = false;
    engine_state.generate_nu_constant();

    // Initialize the stack
    let mut stack = Stack::new();

    // Parse command-line arguments for the -c flag
    let cmd_to_execute = args
        .windows(2)
        .find_map(|w| if w[0] == "-c" { Some(w[1].clone()) } else { None });

    if let Some(cmd) = cmd_to_execute {
        let full_cmd =format!("({}) | to json --raw", cmd);
        // Prepare working set for parsing
        let mut working_set = StateWorkingSet::new(&engine_state);
        let parse_result = parse(&mut working_set, None, full_cmd.as_bytes(), false);

        // Handle parse errors
        if !working_set.parse_errors.is_empty() {
            eprintln!("Parsing errors in command '{}':", full_cmd);
            for e in working_set.parse_errors.iter() {
                eprintln!("  - {} at {:?}", e, e.span());
            }
            std::process::exit(2);
        }

        // Merge parsing delta into engine state
        let delta = working_set.render();
        if let Err(err) = engine_state.merge_delta(delta) {
            eprintln!("Error merging parse results into engine state: {}", err);
            std::process::exit(1);
        }

        // Evaluate the parsed command block
        let eval_fn = get_eval_block(&engine_state);
        match eval_fn(&engine_state, &mut stack, &parse_result, PipelineData::empty()) {
            Ok(pipeline_data) => {
                            // Inside your `Ok(pipeline_data)` arm:
                let json_text = pipeline_data
                .into_value(Span::unknown())
                .map(|v| match v {
                    Value::String { val, .. } => val,
                    other => {
                        // Should never happen if `to json --raw` did its job
                        eprintln!("Expected a JSON string, got {:?}", other.get_type());
                        std::process::exit(1);
                    }
                })
                .map_err(|err| {
                    eprintln!("Error extracting JSON text: {:#?}", err);
                    std::process::exit(1);
                })
                .unwrap();

                // Print *only* the JSON text
                println!("{}", json_text);
            }
            Err(err) => {
                report_shell_error(&engine_state, &err);
                if let ShellError::NonZeroExitCode { exit_code, .. } = err {
                    std::process::exit(i32::from(exit_code));
                }
                std::process::exit(1);
            }
        }
    }  else {
        eprintln!("Usage: {} -c 'command'", args.get(0).unwrap_or(&"nushell_runner".to_string()));
        eprintln!("Run a nushell command and display its output or errors.");
        std::process::exit(1);
    }
}

/// Formats and displays detailed shell errors
fn report_shell_error(engine_state: &EngineState, err: &ShellError) {
    eprintln!("Shell Error: {:#?}", err);
    // Additional context could be added here if ShellError exposes spans or labels
}
