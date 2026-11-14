#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nargs {
    Zero,
    One,
    Any,
    Optional,
    Plus,
}

#[derive(Clone, Debug)]
pub struct UserCommand {
    pub name: String,
    /// Script the command was defined in; its replacement text runs in that
    /// script's `s:` context like Vim does.
    pub script_id: Option<rvim_types::ScriptId>,
    pub bang: bool,
    pub nargs: Nargs,
    pub range: bool,
    pub count: bool,
    pub replacement: String,
}

impl UserCommand {
    pub fn expand(
        &self,
        args: &str,
        bang: bool,
        line1: u32,
        line2: u32,
        range: bool,
    ) -> Result<String, String> {
        let args = args.trim();
        match self.nargs {
            Nargs::Zero if !args.is_empty() => {
                return Err(format!("E488: Trailing characters: {args}"))
            }
            Nargs::One if args.is_empty() => {
                return Err(format!("E471: Argument required: {}", self.name))
            }
            Nargs::Plus if args.is_empty() => {
                return Err(format!("E471: Argument required: {}", self.name))
            }
            _ => {}
        }
        let mut out = self.replacement.clone();
        out = out.replace("<bang>", if bang { "!" } else { "" });
        out = out.replace("<args>", args);
        // Vim's <q-args> is the argument text quoted for use in an
        // expression: a double-quoted string with '\' and '"' escaped.
        let mut quoted = String::with_capacity(args.len() + 2);
        quoted.push('"');
        for character in args.chars() {
            if matches!(character, '\\' | '"') {
                quoted.push('\\');
            }
            quoted.push(character);
        }
        quoted.push('"');
        out = out.replace("<q-args>", &quoted);
        out = out.replace(
            "<f-args>",
            &args
                .split_whitespace()
                .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
                .collect::<Vec<_>>()
                .join(" "),
        );
        out = out.replace("<line1>", &line1.to_string());
        out = out.replace("<line2>", &line2.to_string());
        out = out.replace(
            "<range>",
            &if range {
                format!("{line1},{line2}")
            } else {
                String::new()
            },
        );
        Ok(out)
    }
}
