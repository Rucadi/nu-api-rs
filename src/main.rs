use nu_cmd_extra;
use nu_engine::eval_call;
use nu_engine::get_eval_block;
use nu_parser::parse;
use nu_protocol::engine::{EngineState, Stack, StateWorkingSet};
use nu_protocol::{PipelineData, ShellError, Span, Value};
use nu_std::load_standard_library;
use serde::Serialize;
use std::collections::HashMap;
mod custom_exit;
use nu_protocol::debugger::WithoutDebug;
#[derive(Debug, Serialize)]
pub struct EvalResult {
    pub output: serde_json::Value,
    pub exit_code: i32,
    pub errors: Vec<String>,
}

pub fn get_engine_state() -> EngineState {
    let engine_state = nu_cmd_lang::create_default_context();
    let engine_state = nu_cmd_plugin::add_plugin_command_context(engine_state);
    let engine_state = nu_command::add_shell_command_context(engine_state);
    let engine_state = nu_cmd_extra::add_extra_command_context(engine_state);
    let engine_state = nu_cli::add_cli_context(engine_state);
    let engine_state = nu_explore::add_explore_context(engine_state);
    let mut engine_state = nu_explore::add_explore_context(engine_state);

    let delta = {
        let mut working_set = StateWorkingSet::new(&engine_state);
        working_set.add_decl(Box::new(nu_cli::NuHighlight));
        working_set.add_decl(Box::new(nu_cli::Print));
        working_set.add_decl(Box::new(custom_exit::CustomExit));
        working_set.render()
    };
    let _ = engine_state.merge_delta(delta);
    let _ = load_standard_library(&mut engine_state);

    engine_state.is_interactive = false;
    engine_state.is_login = false;
    engine_state.history_enabled = false;
    engine_state.generate_nu_constant();
    engine_state
}

pub fn evaluate_command(command: &str, env_vars: HashMap<String, String>) -> EvalResult {
    let mut engine_state = get_engine_state();
    let mut stack = Stack::new();
    let mut errors = Vec::new();

    for (k, v) in env_vars.iter() {
        engine_state.add_env_var(k.clone(), Value::string(v.clone(), Span::unknown()));
    }

    let result = run_parse_and_eval(&mut engine_state, &mut stack, command, &mut errors);

    let (output, exit_code) = match result {
        Ok(value) => {
            let json_data = match value {
                Value::String { val, .. } => val,
                other => {
                    eprintln!("Expected a JSON string, got {:?}", other.get_type());
                    std::process::exit(1);
                }
            };

            (serde_json::from_str(&json_data).unwrap(), i32::from(0))
        }
        Err(code) => (serde_json::from_str("null").unwrap(), i32::from(code)),
    };

    EvalResult {
        output,
        exit_code,
        errors,
    }
}
fn run_parse_and_eval(
    engine_state: &mut EngineState,
    stack: &mut Stack,
    full_cmd: &str,
    errors: &mut Vec<String>,
) -> Result<Value, i32> {
    // Step 1: Parse and evaluate the input command
    let mut working_set = StateWorkingSet::new(engine_state);
    let parse_result = parse(&mut working_set, None, full_cmd.as_bytes(), false);

    if !working_set.parse_errors.is_empty() {
        for e in &working_set.parse_errors {
            errors.push(format!("Parse error: {} at {:?}", e, e.span()));
        }
        return Err(2);
    }

    let delta = working_set.render();
    if let Err(err) = engine_state.merge_delta(delta) {
        errors.push(format!("Delta merge error: {:#?}", err));
        return Err(1);
    }

    let eval_fn = get_eval_block(engine_state);
    let value = match eval_fn(engine_state, stack, &parse_result, PipelineData::empty()) {
        Ok(pipeline_data) => pipeline_data.into_value(Span::unknown()).map_err(|e| {
            errors.push(format!("Value extraction error: {:#?}", e));
            1
        })?,
        Err(err) => {
            match err {
                ShellError::Return { span: _, value } => *value, // Extract the value from Return
                ShellError::NonZeroExitCode { exit_code, .. } => {
                    errors.push(format!("Shell execution error: {:#?}", err));
                    return Err(i32::from(exit_code));
                }
                _ => {
                    errors.push(format!("Shell execution error: {:#?}", err));
                    return Err(1);
                }
            }
        }
    };

    // Step 2: Convert the resulting Value to JSON using the `to json` command
    let to_json_cmd = engine_state.find_decl(b"to json", &[]).ok_or_else(|| {
        errors.push("Could not find 'to json' command".to_string());
        1
    })?;

    let mut stack = Stack::new();
    let call = nu_protocol::ast::Call {
        decl_id: to_json_cmd,
        arguments: vec![],
        head: Span::unknown(),
        parser_info: HashMap::new(),
    };

    let pipeline_data = PipelineData::Value(value, None);
    match eval_call::<WithoutDebug>(engine_state, &mut stack, &call, pipeline_data) {
        Ok(pipeline_data) => pipeline_data.into_value(Span::unknown()).map_err(|e| {
            errors.push(format!("JSON conversion error: {:#?}", e));
            1
        }),
        Err(err) => {
            errors.push(format!("JSON conversion error: {:#?}", err));
            Err(1)
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "-c") {
        if args.len() > pos + 1 {
            let cmd = &args[pos + 1];
            let mut env_vars = HashMap::new();
            for (k, v) in std::env::vars() {
                env_vars.insert(k, v);
            }
            // if windows add PWD to env_vars
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
