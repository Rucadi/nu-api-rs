
use nu_cmd_extra;
use nu_engine::{eval_call, get_eval_block};
use nu_parser::parse;
use nu_protocol::engine::{EngineState, Stack, StateWorkingSet};
use nu_protocol::{PipelineData, ShellError, Span, Value};
use nu_std::load_standard_library;
use serde::Serialize;
use std::collections::HashMap;
use nu_protocol::debugger::WithoutDebug;
mod custom_exit;

#[derive(Debug, Serialize)]
pub struct EvalResult {
    pub output: serde_json::Value,
    pub exit_code: i32,
    pub error: serde_json::Value,
}

/// Builds and configures a fresh EngineState
fn get_engine_state() -> EngineState {
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

/// Parses and evaluates a command, collecting errors if any
fn run_parse_and_eval(
    engine: &mut EngineState,
    stack: &mut Stack,
    cmd: &str,
) -> Result<Value, (i32, serde_json::Value)> {
    let mut ws = StateWorkingSet::new(engine);
    let parse_result = parse(&mut ws, None, cmd.as_bytes(), false);

    if let Some(err) = ws.parse_errors.first() {
        let msg = format!("Parse error: {} at {:?}", err, err.span());
        return Err((2, serde_json::json!(msg)));
    }
    engine.merge_delta(ws.render()).map_err(|e| (1, serde_json::json!(format!("Delta merge error: {:#?}", e))))?;

    let eval_fn = get_eval_block(engine);
    match eval_fn(engine, stack, &parse_result, PipelineData::empty()) {
        Ok(data) => data.into_value(Span::unknown()).map_err(|e| (1, serde_json::json!(format!("Value extraction error: {:#?}", e)))),
        Err(err) => match err {
            ShellError::Return { span: _, value } => Ok(*value),
            ShellError::NonZeroExitCode { exit_code, .. } => {
                Err((exit_code.into(), serde_json::from_str(&serde_json::to_string(&err).unwrap_or_default()).unwrap()))
            }
            _ => {
                Err((1, serde_json::from_str(&serde_json::to_string(&err).unwrap_or_default()).unwrap()))
            }
        }
     
    }
}

/// Converts a Value into JSON via 'to json' command
fn convert_to_json(engine: &mut EngineState, val: Value) -> Result<serde_json::Value, (i32, serde_json::Value)> {
    let mut stack = Stack::new();
    let decl = engine
        .find_decl(b"to json", &[])
        .ok_or((1, serde_json::json!("Could not find 'to json' command")))?;

    let call = nu_protocol::ast::Call {
        decl_id: decl,
        arguments: vec![],
        head: Span::unknown(),
        parser_info: Default::default(),
    };
    match eval_call::<WithoutDebug>(engine, &mut stack, &call, PipelineData::Value(val, None)) {
        Ok(pd) => pd
            .into_value(Span::unknown())
            .map_err(|e| (1, serde_json::json!(format!("JSON conversion error: {:#?}", e))))
            .and_then(|v| {
                match v {
                    Value::String { val, .. } => serde_json::from_str(&val).map_err(|e| (1, serde_json::json!(e.to_string()))),
                    other => Err((1, serde_json::json!(format!("Expected JSON string, got {:?}", other.get_type())))),
                }
            }),
        Err(err) => Err((1, serde_json::json!(format!("JSON conversion error: {:#?}", err)))),
    }
}

/// Evaluates a Nushell command string and returns JSON result
pub fn evaluate_command(command: &str, env_vars: HashMap<String, String>) -> EvalResult {
    let mut engine = get_engine_state();
    for (k, v) in env_vars {
        engine.add_env_var(k, Value::string(v, Span::unknown()));
    }

    let mut stack = Stack::new();
    let (output, exit_code, error) = match run_parse_and_eval(&mut engine, &mut stack, command) {
        Ok(val) => match convert_to_json(&mut engine, val) {
            Ok(json) => (json, 0, serde_json::Value::Null),
            Err((code, err)) => (serde_json::Value::Null, code, err),
        },
        Err((code, err)) => (serde_json::Value::Null, code, err),
    };

    EvalResult { output, exit_code, error }
}

pub fn eval_result_to_json(result: &EvalResult) -> serde_json::Value {
    serde_json::json!({
        "output": result.output,
        "exit_code": result.exit_code,
        "error": result.error
    })
}