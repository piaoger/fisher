// Copyright (C) 2016 Pietro Albini
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::process;
use std::os::unix::process::ExitStatusExt;
use std::fs;
use std::env;
use std::path::PathBuf;
use std::io::Write;

use hooks::Hook;
use utils;
use web::requests::Request;
use providers::HookProvider;
use errors::{ErrorKind, FisherResult};


lazy_static! {
    static ref DEFAULT_ENV: Vec<String> = vec![
        "PATH".to_string(),
        "USER".to_string(),
        "SHELL".to_string(),

        // Internationalization stuff
        "LC_ALL".to_string(),
        "LANG".to_string(),
    ];
}


#[derive(Clone)]
pub struct Job {
    hook: Hook,
    provider: Option<HookProvider>,
    request: Request,
}

impl Job {

    pub fn new(hook: Hook, provider: Option<HookProvider>, request: Request)
               -> Job {
        Job {
            hook: hook,
            provider: provider,
            request: request,
        }
    }

    pub fn hook_name(&self) -> &str {
        self.hook.name()
    }

    pub fn process(&self) -> FisherResult<()> {
        let mut command = process::Command::new(&self.hook.exec());

        // Prepare the command's environment variables
        self.prepare_env(&mut command);

        // Use a random working directory
        let working_directory = try!(utils::create_temp_dir());
        command.current_dir(working_directory.to_str().unwrap());
        command.env("HOME".to_string(), working_directory.to_str().unwrap());

        // Save the request body
        let request_body = try!(self.save_request_body(&working_directory));
        command.env(
            "FISHER_REQUEST_BODY".to_string(),
            request_body.to_str().unwrap().to_string()
        );

        // Execute the hook
        let output = try!(command.output());
        if ! output.status.success() {
            return Err(ErrorKind::HookExecutionFailed(
                output.status.code(),
                output.status.signal(),
            ).into());
        }

        // Remove the temp directory
        try!(fs::remove_dir_all(&working_directory));

        Ok(())
    }

    fn prepare_env(&self, command: &mut process::Command) {
        // First of all clear the environment
        command.env_clear();

        // Apply the default environment
        // This is done (instead of the automatic inheritage) to whitelist
        // which environment variables we want
        for (key, value) in env::vars() {
            // Set only whitelisted keys
            if ! DEFAULT_ENV.contains(&key) {
                continue;
            }

            command.env(key, value);
        }

        // Apply the hook-specific environment
        if let Some(ref provider) = self.provider {
            for (key, value) in provider.env(&self.request) {
                let real_key = format!(
                    "FISHER_{}_{}", provider.name().to_uppercase(), key
                );
                command.env(real_key, value);
            }
        }
    }

    fn save_request_body(&self, base: &PathBuf) -> FisherResult<PathBuf> {
        let mut path = base.clone();
        path.push("request_body");

        // Write the request body on disk
        let mut file = try!(fs::File::create(&path));
        try!(write!(file, "{}\n", self.request.body));

        Ok(path)
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::env;

    use hooks;
    use web::requests;

    use utils::testing::*;
    use utils;

    use super::{DEFAULT_ENV, Job};


    struct TestEnv {
        to_delete: Vec<String>,
        hooks: HashMap<String, hooks::Hook>,
    }

    impl TestEnv {

        fn new() -> Self {
            let hooks_dir = sample_hooks().to_str().unwrap().to_string();
            let hooks = hooks::collect(&hooks_dir).unwrap();

            TestEnv {
                to_delete: vec![hooks_dir],
                hooks: hooks,
            }
        }

        fn create_job(&self, hook_name: &str, req: requests::Request) -> Job {
            // Get the JobHook
            let hook = self.hooks.get(&hook_name.to_string()).unwrap();
            let (_, provider) = hook.validate(&req);

            Job::new(hook.clone(), provider, req)
        }

        fn cleanup(&self) {
            for dir in &self.to_delete {
                let _ = fs::remove_dir_all(dir);
            }
        }

        fn delete_also(&mut self, path: &str) {
            self.to_delete.push(path.to_string());
        }
    }


    #[test]
    fn test_job_creation() {
        let env = TestEnv::new();

        let _ = env.create_job("example", dummy_request());

        env.cleanup();
    }


    #[test]
    fn test_job_hook_name() {
        let env = TestEnv::new();

        let job = env.create_job("example", dummy_request());
        assert_eq!(job.hook_name(), "example".to_string());

        env.cleanup();
    }

    #[test]
    fn test_job_execution() {
        let env = TestEnv::new();

        // The "example" hook should be processed without problems
        let job = env.create_job("example", dummy_request());
        assert!(job.process().is_ok());

        let job = env.create_job("failing", dummy_request());
        assert!(job.process().is_err());

        env.cleanup();
    }

    #[test]
    fn test_job_environment() {
        macro_rules! read {
            ($output:expr, $name:expr) => {{
                use std::fs;
                use std::io::Read;

                let file_name = format!("{}/{}", $output, $name);
                let mut file = fs::File::open(&file_name).unwrap();

                let mut buf = String::new();
                file.read_to_string(&mut buf).unwrap();

                buf
            }};
        }

        let mut env = TestEnv::new();

        // Create a temp directory which will contain the build
        let output_path = utils::create_temp_dir().unwrap();
        let output = output_path.to_str().unwrap();
        env.delete_also(&output);

        // Create a new dummy request
        let mut req = dummy_request();
        req.body = "a body!".to_string();
        req.params.insert("env".to_string(), output.to_string());

        // Process the job
        let job = env.create_job("jobs-details", req);
        assert!(job.process().is_ok());

        // The hook must be executed
        assert_eq!(read!(output, "executed"), "executed\n".to_string());

        // The request body must be present
        assert_eq!(read!(output, "request_body"), "a body!\n".to_string());

        // Get the used working directory
        let pwd_raw = read!(output, "pwd");
        let working_directory = pwd_raw.trim();

        // Parse the environment file
        let raw_env = read!(output, "env");
        let job_env = utils::parse_env(&raw_env).unwrap();

        // Get all the required environment variables
        let mut required_env = {
            let mut res: Vec<&str> = DEFAULT_ENV.iter().map(|i| {
                i.as_str()
            }).collect();

            // Those are from the provider
            res.push("FISHER_TESTING_ENV");

            // Those are added by the processor
            res.push("HOME");
            res.push("FISHER_REQUEST_BODY");

            // Those are extra variables added by bash
            res.push("PWD");
            res.push("SHLVL");
            res.push("_");

            res
        };

        // Check if the right environment variables are present
        let mut found = vec![];
        for (key, _) in &job_env {
            if required_env.contains(key) {
                found.push(key);
            } else {
                panic!("Extra env variable: {}", key);
            }
        }
        assert_eq!(required_env.sort(), found.sort());

        // The env var generated from the provider must be present
        assert_eq!(
            *job_env.get("FISHER_TESTING_ENV").unwrap(),
            output.to_string()
        );

        // $HOME must be the current directory
        assert_eq!(
            *job_env.get("HOME").unwrap(),
            working_directory
        );

        // The value of the environment variables forwarded from the current
        // env must have the same content of the current env
        for key in DEFAULT_ENV.iter() {
            assert_eq!(
                env::var(key).unwrap().as_str(),
                *job_env.get(key.as_str()).unwrap());
        }

        env.cleanup();
    }
}
