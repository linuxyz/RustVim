import pathlib
p = pathlib.Path("crates/rvim-editor/src/editor.rs")
s = p.read_text()
old = """        let shell = self.options.get_string("shell");
        let flag = self.options.get_string("shellcmdflag");
        let xquote = self.options.get_string("shellxquote");
        let quoted = if xquote.is_empty() {
            command.to_string()
        } else {
            format!("{xquote}{command}{xquote}")
        };
        let result = rvim_platform::run_shell_command(&shell, &flag, &quoted, input);"""
new = """        let shell = self.options.get_string("shell");
        let flag = self.options.get_string("shellcmdflag");
        let shellquote = self.options.get_string("shellquote");
        let xquote = self.options.get_string("shellxquote");
        let quoted = quote_shell_command(command, &shellquote, &xquote);
        let result = rvim_platform::run_shell_command(&shell, &flag, &quoted, input);"""
assert old in s
s = s.replace(old, new, 1)
s += '''

fn quote_shell_command(command: &str, shellquote: &str, xquote: &str) -> String {
    let quote = if shellquote.is_empty() {
        xquote
    } else {
        shellquote
    };
    if quote.is_empty() {
        command.to_string()
    } else {
        format!("{quote}{command}{quote}")
    }
}

#[cfg(test)]
mod shell_quote_tests {
    use super::quote_shell_command;

    #[test]
    fn shell_quote_matrix() {
        assert_eq!(quote_shell_command("echo hi", "", ""), "echo hi");
        assert_eq!(
            quote_shell_command("echo hi", "", "\\""),
            "\\"echo hi\\""
        );
        assert_eq!(
            quote_shell_command("echo hi", "'", "\\""),
            "'echo hi'"
        );
        assert_eq!(
            quote_shell_command("echo hi", "", "'"),
            "'echo hi'"
        );
    }
}
'''
p.write_text(s)
print("patched")
