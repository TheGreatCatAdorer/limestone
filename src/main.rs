use std::fmt::Write;
use std::path::Path;

type Subst = Vec<Result<String, String>>;

struct Command {
    func: Subst,
    args: Vec<Subst>,
    dest: String,
}

fn parse(string: String, actions: &mut Vec<Command>) {
    for line in string.lines() {
        let (variable, rest) = parse_word(line, '=');
        let (action, mut rest) = parse_word(rest, '(');
        let mut args = Vec::new();
        while let Some(i) = rest.find(',') {
            args.push((&rest[0..i]).to_string());
            rest = &rest[i..rest.len()];
        }
        let end = rest.find(')').unwrap_or_else(|| rest.len());
        args.push((&rest[0..end]).to_string());
        actions.push(Command {
            func: parse_subst(action),
            args: args.iter().map(|str| parse_subst(str)).collect(),
            dest: variable.to_string(),
        });
    }
}

fn parse_word(string: &str, delim: char) -> (&str, &str) {
    let delim = string.find(delim).unwrap_or(string.len());
    ((&string[0..delim]).trim(), &string[delim..string.len()])
}

fn parse_subst(string: &str) -> Subst {
    let mut result = Vec::new();
    let mut chars = string.chars();
    let mut acc = String::new();
    while let Some(c) = chars.next() {
        if c == '{' {
            result.push(Ok(acc));
            acc = String::new();
        } else if c == '}' {
            result.push(Err(encode_var(&acc)));
            acc = String::new();
        } else {
            acc.push(c);
        }
    }
    if acc != "" {
        result.push(Ok(acc));
    }
    return result;
}

const HEADER: &str = "#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>
#include <spawn.h>
char *run(const char *restrict file, char *const argv[restrict]){
int status, len, procout[2], pid_t pid, posix_spawn_file_actions_t acts, char *result;
if (status = pipe(procout)) exit(status);
posix_spawn_file_actions_init(&acts);
posix_spawn_file_actions_adddup2(&acts, procout[1], STDOUT_FILENO);
if (status = posix_spawnp(&pid, file, &acts, NULL, argv, environ)) exit(status);
while (waitpid(pid, &status, 0) == -1);
posix_spawn_file_actions_destroy(&acts);
if (status = WEXITSTATUS(status)) exit(status);
result = malloc(len = lseek(procout[0], 0, SEEK_END));
lseek(procout[0], 0, SEEK_SET);
if (result == NULL) exit(1);
fread(result, 1, len, procout[0]);
close(procout[0]); close(procout[1]);
return result;}
";

fn output(actions: Vec<Command>) -> String {
    let mut result = HEADER.to_string();
    for Command { func, args, dest } in actions {
        for (i, subst) in args.iter().enumerate() {
            conc(subst, &format!("local_{i}"), &mut result);
        }
        conc(&func, "func", &mut result);
        result.push_str("char *");
        result.push_str(&dest);
        result.push_str(" = run(func,");
        let mut args = 0..args.len();
        while let Some(i) = args.next() {
            write!(&mut result, "local_{i}").unwrap();
            result.push(if args.len() == 0 { ')' } else { ',' });
        }
    }
    result
}

fn write_literal(subst: &Result<String, String>, buf: &mut String) {
    match subst {
        Ok(lit) => {
            buf.push('"');
            escape_string_to(lit, buf);
            buf.push('"');
        }
        Err(var) => buf.push_str(var),
    }
}

fn decl_str(dest: &str, string: &Result<String, String>) -> String {
    let mut result = "char *".to_string();
    result.push_str(dest);
    result.push_str(" = ");
    write_literal(&string, &mut result);
    result.push(';');
    return result;
}

fn format_list<T>(mut write: impl FnMut(&T, &mut String) -> (), args: &[T], buf: &mut String) {
    let mut items = args.iter();
    while let Some(item) = items.next() {
        write(item, buf);
        buf.push(',');
        if items.len() == 0 {
            buf.push_str("NULL};");
        }
    }
}

fn conc(subst: &Subst, dest: &str, mut result: &mut String) {
    if let [string] = &subst[..] {
        result.push_str(&decl_str(dest, &string));
        return;
    }
    result.push_str("int *lengths = {");
    fn write_length(item: &Result<String, String>, buf: &mut String) {
        buf.push_str("strlen(");
        write_literal(item, buf);
        buf.push_str(")");
    }
    format_list(write_length, &subst, &mut result);
    result.push_str("char **substrings = {");
    format_list(write_literal, &subst, &mut result);
    write!(
        &mut result,
        "int length = 0; for (int i = 0; i < {};) length += lengths[i++]",
        subst.len()
    )
    .unwrap();
    write!(
        &mut result,
        "char *{dest} = malloc(length); if ({dest} == NULL) exit(1);\
        for (int i = 0, char *ref = {dest}; i < {};)\
        {{ memcpy(ref, substrings[i], lengths[i]); ref += lengths[i++]; }}",
        subst.len()
    )
    .unwrap();
}

fn encode_var(string: &str) -> String {
    let mut result = "var_".to_string();
    for char in string.chars() {
        match char {
            'a'..='z' | 'A'..='Z' => result.push(char),
            _ => {
                result.push('_');
                let mut hex = char as u32;
                while hex != 0 {
                    let low = hex & 15;
                    result.push(unsafe {
                        match low {
                            0..=9 => char::from_u32_unchecked('0' as u32 + low),
                            10..=16 => char::from_u32_unchecked('A' as u32 - 10 + low),
                            _ => unreachable!(),
                        }
                    });
                    hex >>= 4;
                }
                result.push('_');
            }
        }
    }
    result
}

fn escape_string_to(string: &str, buf: &mut String) {
    for char in string.chars() {
        match char {
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '"' => buf.push_str("\\\""),
            _ => buf.push(char),
        }
    }
}

fn main() {
    let mut args = std::env::args();
    args.next();
    let mut actions = Vec::new();
    for arg in args {
        let contents = std::fs::read_to_string(Path::new(&arg)).expect("file not found!");
        parse(contents, &mut actions);
    }
    println!("{}", output(actions));
}
