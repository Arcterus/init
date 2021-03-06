#![deny(warnings)]

extern crate syscall;

use std::env;
use std::fs::{File, read_dir};
use std::io::{Read, Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;
use std::process::Command;
use syscall::flag::{O_RDONLY, O_WRONLY};

fn switch_stdio(stdio: &str) -> Result<()> {
    let stdin = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_RDONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };
    let stdout = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };
    let stderr = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };

    syscall::dup2(stdin.as_raw_fd(), 0, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stdout.as_raw_fd(), 1, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stderr.as_raw_fd(), 2, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;

    Ok(())
}

pub fn run(file: &Path) -> Result<()> {
    let mut data = String::new();
    File::open(file)?.read_to_string(&mut data)?;

    for line in data.lines() {
        let line = line.trim();
        if ! line.is_empty() && ! line.starts_with('#') {
            let mut args = line.split(' ').map(|arg| if arg.starts_with('$') {
                env::var(&arg[1..]).unwrap_or(String::new())
            } else {
                arg.to_string()
            });

            if let Some(cmd) = args.next() {
                match cmd.as_str() {
                    "cd" => if let Some(dir) = args.next() {
                        if let Err(err) = env::set_current_dir(&dir) {
                            println!("init: failed to cd to '{}': {}", dir, err);
                        }
                    } else {
                        println!("init: failed to cd: no argument");
                    },
                    "echo" => {
                        if let Some(arg) = args.next() {
                            print!("{}", arg);
                        }
                        for arg in args {
                            print!(" {}", arg);
                        }
                        print!("\n");
                    },
                    "pipeless" => if let Some(path) = args.next() {
                        let mut command = Command::new(path);
                        for arg in args {
                            command.arg(arg);
                        }

                        match unsafe { syscall::clone(0) } {
                            Ok(0) => {
                                if let Err(err) = command.exec() {
                                    println!("init: failed to spawn '{}' without pipes: {}", line, err);
                                }
                                let _ = syscall::exit(1);
                                panic!("failed to exit");
                            }
                            Ok(pid) => {
                                let mut status = 0;
                                let _ = syscall::waitpid(pid, &mut status, 0);
                            }
                            Err(err) => {
                                println!("init: failed to spawn '{}' without pipes: could not clone init", line);
                            }
                        }
                    } else {
                        println!("init: failed to spawn without pipes: no argument");
                    }
                    "export" => if let Some(var) = args.next() {
                        let mut value = String::new();
                        if let Some(arg) = args.next() {
                            value.push_str(&arg);
                        }
                        for arg in args {
                            value.push(' ');
                            value.push_str(&arg);
                        }
                        env::set_var(var, value);
                    } else {
                        println!("init: failed to export: no argument");
                    },
                    "run" => if let Some(new_file) = args.next() {
                        if let Err(err) = run(&Path::new(&new_file)) {
                            println!("init: failed to run '{}': {}", new_file, err);
                        }
                    } else {
                        println!("init: failed to run: no argument");
                    },
                    "run.d" => if let Some(new_dir) = args.next() {
                        let mut entries = vec![];
                        match read_dir(&new_dir) {
                            Ok(list) => for entry_res in list {
                                match entry_res {
                                    Ok(entry) => {
                                        entries.push(entry.path());
                                    },
                                    Err(err) => {
                                        println!("init: failed to run.d: '{}': {}", new_dir, err);
                                    }
                                }
                            },
                            Err(err) => {
                                println!("init: failed to run.d: '{}': {}", new_dir, err);
                            }
                        }

                        entries.sort();

                        for entry in entries {
                            if let Err(err) = run(&entry) {
                                println!("init: failed to run '{}': {}", entry.display(), err);
                            }
                        }
                    } else {
                        println!("init: failed to run.d: no argument");
                    },
                    "stdio" => if let Some(stdio) = args.next() {
                        if let Err(err) = switch_stdio(&stdio) {
                            println!("init: failed to switch stdio to '{}': {}", stdio, err);
                        }
                    } else {
                        println!("init: failed to set stdio: no argument");
                    },
                    _ => {
                        let mut command = Command::new(cmd);
                        for arg in args {
                            command.arg(arg);
                        }

                        match command.spawn() {
                            Ok(mut child) => match child.wait() {
                                Ok(_status) => (), //println!("init: waited for {}: {:?}", line, status.code()),
                                Err(err) => println!("init: failed to wait for '{}': {}", line, err)
                            },
                            Err(err) => println!("init: failed to execute '{}': {}", line, err)
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub fn main() {
    if let Err(err) = run(&Path::new("initfs:etc/init.rc")) {
        println!("init: failed to run initfs:etc/init.rc: {}", err);
    }

    syscall::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
