import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
anchor = """    pub(crate) fn execute_ex_line(&mut self, line: &str) -> Result<(), VimError> {
        crate::ex::execute(self, line)
    }
"""
helper = """    pub(crate) fn execute_ex_line(&mut self, line: &str) -> Result<(), VimError> {
        crate::ex::execute(self, line)
    }

    pub(crate) fn set_shell_error(&mut self, status: i32) {
        self.viml
            .set_vvar("shell_error", rvim_viml::Value::Number(status as i64));
    }

    pub(crate) fn run_shell(
        &mut self,
        command: &str,
        input: Option<&[u8]>,
    ) -> Result<rvim_platform::CommandOutput, rvim_platform::ProcessError> {
        let shell = self.options.get_string("shell");
        let flag = self.options.get_string("shellcmdflag");
        let xquote = self.options.get_string("shellxquote");
        let quoted = if xquote.is_empty() {
            command.to_string()
        } else {
            format!("{xquote}{command}{xquote}")
        };
        let result = rvim_platform::run_shell_command(&shell, &flag, &quoted, input);
        if let Ok(output) = &result {
            self.set_shell_error(output.status);
        }
        result
    }
"""
assert anchor in s
s = s.replace(anchor, helper, 1)
p.write_text(s)
print("editor patched")
p = pathlib.Path("crates/rvim-editor/src/ex.rs")
s = p.read_text()
s = s.replace("rvim_platform::run_shell(&command_text, None)", "editor.run_shell(&command_text, None)")
s = s.replace("rvim_platform::run_shell(shell, Some(&bytes))", "editor.run_shell(shell, Some(&bytes))")
s = s.replace("rvim_platform::run_shell(&shell, None)", "editor.run_shell(&shell, None)")
p.write_text(s)
print("ex patched")
