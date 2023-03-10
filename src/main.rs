use std::fmt::Write;
use std::path::Path;

type Subst = Vec<Result<String, String>>;

enum Dest {
    Var(String),
    Stdout,
}

struct Command {
    func: Subst,
    args: Vec<Subst>,
    dest: Dest,
}

fn parse(string: &str, actions: &mut Vec<Command>) {
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
            dest: match variable {
                "%" => Dest::Stdout,
                _ => Dest::Var(variable.to_string()),
            },
        });
    }
}

fn parse_word(string: &str, delim: char) -> (&str, &str) {
    let delim = string.find(delim).unwrap_or(string.len());
    ((&string[0..delim]).trim(), &string[delim + 1..string.len()])
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

const HEADER: &str = "#define _GNU_SOURCE
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <sys/mman.h>
#include <unistd.h>
#include <spawn.h>
extern char**environ;
char*run(const char*restrict file,char*const argv[restrict]){
int status,len,output;pid_t pid;posix_spawn_file_actions_t acts;char*result;
if((output=memfd_create(\"run_output\",0))==-1)exit(1);
posix_spawn_file_actions_init(&acts);
posix_spawn_file_actions_adddup2(&acts,output,STDOUT_FILENO);
if((status=posix_spawnp(&pid,file,&acts,NULL,argv,environ)))exit(status);
while(waitpid(pid,&status,0)==-1){}
posix_spawn_file_actions_destroy(&acts);
if((status=WEXITSTATUS(status)))exit(status);
result=malloc(len=lseek(output,0,SEEK_END));
lseek(output,0,SEEK_SET);
if(result==NULL&&len!=0)exit(1);
fread(result,1,len,fdopen(output, \"r\"));
close(output);close(output);
return result;}
int main(void){";

fn output(actions: Vec<Command>) -> String {
    let mut result = HEADER.to_string();
    for Command { func, args, dest } in actions {
        result.push('\n');
        result.push('{');
        for (i, subst) in args.iter().enumerate() {
            conc(subst, &format!("local_{i}"), &mut result);
        }
        conc(&func, "func", &mut result);
        write!(&mut result, "char*args[{}]=", args.len() + 1).unwrap();
        format_list(
            |i, buf| write!(buf, "local_{i}").unwrap(),
            0..args.len(),
            "(char*)NULL",
            &mut result,
        );
        match dest {
            Dest::Stdout => result.push_str("puts(run(func, args))"),
            Dest::Var(var) => write!(&mut result, "char*{var}=run(func,args)").unwrap(),
        }
        result.push(';');
        result.push('}');
    }
    result.push('}');
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
    let mut result = "char*".to_string();
    result.push_str(dest);
    result.push('=');
    write_literal(&string, &mut result);
    result.push(';');
    return result;
}

fn format_list<T>(
    mut write: impl FnMut(T, &mut String) -> (),
    mut items: impl Iterator<Item = T>,
    suffix: &str,
    buf: &mut String,
) {
    buf.push('{');
    while let Some(item) = items.next() {
        write(item, buf);
        buf.push(',');
    }
    buf.push_str(suffix);
    buf.push_str("};");
}

fn conc(subst: &Subst, dest: &str, mut result: &mut String) {
    if let [string] = &subst[..] {
        result.push_str(&decl_str(dest, &string));
        return;
    }
    write!(&mut result, "int lengths[{}]=", subst.len() + 1).unwrap();
    fn write_length(item: &Result<String, String>, buf: &mut String) {
        buf.push_str("strlen(");
        write_literal(item, buf);
        buf.push(')');
    }
    format_list(write_length, subst.iter(), "0", &mut result);
    write!(&mut result, "char*substrings[{}]=", subst.len() + 1).unwrap();
    format_list(write_literal, subst.iter(), "0", &mut result);
    write!(
        &mut result,
        "int length=0;for(int i=0;i<{};)length+=lengths[i++];",
        subst.len()
    )
    .unwrap();
    write!(
        &mut result,
        "char*{dest}=malloc(length);if({dest}==NULL&&length!=0)exit(1);\
        char*ref={dest};for(int i=0;i<{};)(memcpy(ref,substrings[i],lengths[i]),ref+=lengths[i++]);",
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
        parse(&contents, &mut actions);
    }
    println!("{}", output(actions));
}
