import pathlib
p = pathlib.Path("crates/rvim-viml/src/host.rs")
s = p.read_text()
old = """    fn set_register(&mut self, name: char, value: Value) -> Result<()> {
        let _ = value;
        Err(unsupported(&format!("writing register @{name}")))
    }"""
new = """    fn set_register(&mut self, name: char, value: Value) -> Result<()> {
        let _ = value;
        Err(unsupported(&format!("writing register @{name}")))
    }

    fn get_register_list(&mut self, name: char, as_list: bool) -> Result<Value> {
        let _ = as_list;
        self.get_register(name)
    }

    fn set_register_ex(&mut self, name: char, value: Value, options: &str) -> Result<()> {
        let _ = options;
        self.set_register(name, value)
    }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("host trait patched")
p = pathlib.Path("crates/rvim-viml/src/builtins.rs")
s = p.read_text()
old = """        "getreg" => {
            arity(name, args, 1, Some(1))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            ctx.host.get_register(register)
        }
        "setreg" => {
            arity(name, args, 2, Some(2))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            let value = arg(args, 1).clone();
            ctx.host.set_register(register, value)?;
            Ok(Value::Number(0))
        }"""
new = """        "getreg" => {
            arity(name, args, 1, Some(2))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            let as_list = arg(args, 1)
                .as_bool()
                .unwrap_or(false);
            ctx.host.get_register_list(register, as_list)
        }
        "setreg" => {
            arity(name, args, 2, Some(3))?;
            let register = register_name(&string_arg(arg(args, 0))?)?;
            let value = arg(args, 1).clone();
            let options = arg(args, 2).as_ref().map(|_| string_arg(arg(args, 2))).transpose()?;
            ctx.host
                .set_register_ex(register, value, options.as_deref().unwrap_or(""))?;
            Ok(Value::Number(0))
        }"""
assert old in s
s = s.replace(old, new, 1)
p.write_text(s)
print("builtins patched")
