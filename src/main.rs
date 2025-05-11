use nu_engine::get_eval_block;
use nu_parser::parse;
use nu_protocol::engine::{Stack, StateWorkingSet};
use nu_protocol::{PipelineData, ShellError, Span, Value};
use nu_std::load_standard_library;
use std::env;

fn main() {
    // Collect command-line arguments and current directory
    let args: Vec<String> = env::args().collect();
    let project_path = env::current_dir().expect("Failed to get current directory");

    // Initialize the engine state with standard Nushell command contexts
    let mut engine_state = nu_cmd_lang::create_default_context();
    engine_state = nu_command::add_shell_command_context(engine_state);
    //engine_state = nu_cmd_extra::add_extra_command_context(engine_state);
    engine_state = nu_cli::add_cli_context(engine_state);

    // Set up default environment variables
   // engine_state.add_env_var("config".into(), Config::default().into_value(Span::unknown()));
    engine_state.add_env_var(
        "ENV_CONVERSIONS".to_string(),
        Value::test_record(nu_protocol::record! {}),
    );
    //gather_parent_env_vars(&mut engine_state, &project_path);
    engine_state.add_env_var(
        "NU_VERSION".to_string(),
        Value::string("0.104.0", Span::unknown()), // Hardcoded version; adjust as needed
    );

    // Load the standard library and configure engine flags
    load_standard_library(&mut engine_state).expect("Failed to load standard library");
    engine_state.is_interactive = false;
    engine_state.is_login = false;
    engine_state.history_enabled = false;
    engine_state.generate_nu_constant();

    // Initialize the stack
    let mut stack = Stack::new();

    // Parse command-line arguments for the -c flag
    let mut cmd_to_execute = None;
    let mut args_iter = args.iter().skip(1);
    while let Some(arg) = args_iter.next() {
        if arg == "-c" {
            if let Some(cmd) = args_iter.next() {
                cmd_to_execute = Some(cmd.clone());
                break;
            }
        }
    }

    // Execute the command if provided, otherwise exit with an error
    if let Some(cmd) = cmd_to_execute {
        // Parse the command
        let mut working_set = StateWorkingSet::new(&engine_state);
        let block = parse(&mut working_set, None, cmd.as_bytes(), false);

        // Check for parse errors
        if !working_set.parse_errors.is_empty() {
            for err in working_set.parse_errors {
                //report_parse_error(&working_set, &err);
            }
            std::process::exit(1);
        }

        // Merge parsing delta into engine state
        let delta = working_set.render();
        engine_state.merge_delta(delta).expect("Failed to merge delta");

        // Evaluate and print the command output
        let eval_fn = get_eval_block(&engine_state);
        let result = eval_fn(&engine_state, &mut stack, &block, PipelineData::empty());
        match result {
            Ok(pipeline_data) => {
                pipeline_data
                    .print_table(&engine_state, &mut stack, false, false)
                    .unwrap_or_else(|err| {
                        //report_shell_error(&engine_state, &err);
                        std::process::exit(1);
                    });
            }
            Err(err) => {
                //report_shell_error(&engine_state, &err);
                match err {
                    ShellError::NonZeroExitCode { exit_code, .. } => std::process::exit(i32::from(exit_code)),
                    _ => std::process::exit(1),
                }
            }
        }
    } else {
        println!("No command provided. Use -c 'command' to execute a Nushell command.");
        std::process::exit(1);
    }
}