use nu_engine::CallExt;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{PipelineData, ShellError, Signature, SyntaxShape};
use std::num::NonZero;
#[derive(Clone)]
pub struct CustomExit;

impl Command for CustomExit {
    fn name(&self) -> &str {
        "exit"
    }

    fn description(&self) -> &str {
        "Exit with a status code (custom implementation)"
    }

    fn signature(&self) -> Signature {
        Signature::build("exit").optional("code", SyntaxShape::Int, "Exit code (defaults to 0)")
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &nu_engine::command_prelude::Call<'_>,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let exit_code: i32 = call.opt(engine_state, stack, 0)?.unwrap_or(0);
        Err(ShellError::NonZeroExitCode {
            exit_code: NonZero::new(exit_code).unwrap(),
            span: call.head,
        })
    }
}
