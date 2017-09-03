// Copyright (C) 2016-2017 Pietro Albini
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

use std::fs::{read_dir, canonicalize, ReadDir};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, VecDeque};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, RwLock};

use common::prelude::*;
use common::state::{State, UniqueId};

use providers::{Provider, StatusEvent, StatusEventKind};
use requests::Request;
use jobs::{Job, JobOutput};

// Temporary migration
pub use scripts::Script as Hook;


pub struct HooksIter {
    inner: Arc<RwLock<HooksInner>>,
    count: usize,
}

impl HooksIter {

    fn new(inner: Arc<RwLock<HooksInner>>) -> Self {
        HooksIter {
            inner,
            count: 0,
        }
    }
}

impl Iterator for HooksIter {
    type Item = Arc<Hook>;

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;

        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }.hooks.get(self.count - 1).cloned()
    }
}


pub struct HookNamesIter {
    iter: HooksIter,
}

impl HookNamesIter {

    fn new(iter: HooksIter) -> Self {
        HookNamesIter {
            iter: iter,
        }
    }
}

impl Iterator for HookNamesIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|hook| hook.name().to_string())
    }
}


pub struct StatusJobsIter {
    inner: Arc<RwLock<HooksInner>>,
    event: StatusEvent,
    count: usize,
}

impl StatusJobsIter {

    fn new(inner: Arc<RwLock<HooksInner>>, event: StatusEvent) -> Self {
        StatusJobsIter {
            inner,
            event,
            count: 0
        }
    }
}

impl Iterator for StatusJobsIter {
    type Item = Job;

    fn next(&mut self) -> Option<Self::Item> {
        self.count += 1;

        let inner = match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(all) = inner.status_hooks.get(&self.event.kind()) {
            if let Some(hp) = all.get(self.count - 1).cloned() {
                Some(Job::new(
                    hp.hook, Some(hp.provider),
                    Request::Status(self.event.clone()),
                ))
            } else {
                None
            }
        } else {
            None
        }
    }
}


#[derive(Debug, Clone)]
pub struct HookProvider {
    pub hook: Arc<Hook>,
    pub provider: Arc<Provider>,
}


#[derive(Debug)]
struct HooksInner {
    hooks: Vec<Arc<Hook>>,
    by_id: HashMap<UniqueId, Arc<Hook>>,
    by_name: HashMap<String, Arc<Hook>>,
    status_hooks: HashMap<StatusEventKind, Vec<HookProvider>>,
}

impl HooksInner {

    pub fn new() -> Self {
        HooksInner {
            hooks: Vec::new(),
            by_id: HashMap::new(),
            by_name: HashMap::new(),
            status_hooks: HashMap::new(),
        }
    }

    pub fn insert(&mut self, hook: Arc<Hook>) {
        self.hooks.push(hook.clone());
        self.by_id.insert(hook.id(), hook.clone());
        self.by_name.insert(hook.name().to_string(), hook.clone());

        for provider in &hook.providers {
            if let Provider::Status(ref status) = *provider.as_ref() {
                // Load all the kinds of events
                for event in status.events() {
                    self.status_hooks.entry(*event)
                        .or_insert_with(Vec::new)
                        .push(HookProvider {
                            hook: hook.clone(),
                            provider: provider.clone(),
                        });
                }
            }
        }
    }

    pub fn get_by_name(&self, name: &str) -> Option<Arc<Hook>> {
        self.by_name.get(name).cloned()
    }
}


#[derive(Debug)]
pub struct Hooks {
    inner: Arc<RwLock<HooksInner>>,
}

impl Hooks {

    pub fn get_by_name(&self, name: &str) -> Option<Arc<Hook>> {
        match self.inner.read() {
            Ok(inner) => inner.get_by_name(name),
            Err(poisoned) => poisoned.get_ref().get_by_name(name),
        }
    }

    pub fn names(&self) -> HookNamesIter {
        HookNamesIter::new(self.iter())
    }
}

impl ScriptsRepositoryTrait for Hooks {
    type Script = Hook;
    type Job = Job;
    type ScriptsIter = HooksIter;
    type JobsIter = StatusJobsIter;

    fn id_exists(&self, id: &UniqueId) -> bool {
        match self.inner.read() {
            Ok(inner) => inner.by_id.contains_key(id),
            Err(poisoned) => poisoned.get_ref().by_id.contains_key(id),
        }
    }

    fn iter(&self) -> HooksIter {
        HooksIter::new(self.inner.clone())
    }

    fn jobs_after_output(&self, output: JobOutput) -> Option<StatusJobsIter> {
        if ! output.trigger_status_hooks {
            return None;
        }

        let event = if output.success {
            StatusEvent::JobCompleted(output)
        } else {
            StatusEvent::JobFailed(output)
        };

        Some(StatusJobsIter::new(self.inner.clone(), event))
    }
}


#[derive(Debug)]
pub struct HooksBlueprint {
    added: Vec<Arc<Hook>>,
    collect_paths: Vec<(PathBuf, bool)>,

    inner: Arc<RwLock<HooksInner>>,
    state: Arc<State>,
}

impl HooksBlueprint {

    pub fn new(state: Arc<State>) -> HooksBlueprint {
        HooksBlueprint {
            added: Vec::new(),
            collect_paths: Vec::new(),

            inner: Arc::new(RwLock::new(HooksInner::new())),
            state: state,
        }
    }

    pub fn insert(&mut self, hook: Arc<Hook>) -> Result<()> {
        self.added.push(hook);

        self.reload()?;
        Ok(())
    }

    pub fn collect_path<P: AsRef<Path>>(&mut self, path: P, recursive: bool)
                                      -> Result<()> {
        self.collect_paths.push((path.as_ref().to_path_buf(), recursive));

        self.reload()?;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        let mut inner = HooksInner::new();

        // Add manually added hooks
        for hook in &self.added {
            inner.insert(hook.clone());
        }

        // Collect hooks from paths
        let mut collector;
        for &(ref p, recursive) in &self.collect_paths {
            collector = HooksCollector::new(p, self.state.clone(), recursive)?;
            for hook in collector {
                inner.insert(hook?);
            }
        }

        {
            let mut to_update = self.inner.write()?;
            *to_update = inner;
        }

        Ok(())
    }

    pub fn hooks(&self) -> Hooks {
        Hooks {
            inner: self.inner.clone(),
        }
    }
}


pub struct HooksCollector {
    dirs: VecDeque<ReadDir>,
    state: Arc<State>,
    base: PathBuf,
    recursive: bool,
}

impl HooksCollector {

    pub fn new<P: AsRef<Path>>(base: P, state: Arc<State>, recursive: bool)
                               -> Result<Self> {
        let mut dirs = VecDeque::new();
        dirs.push_front(read_dir(&base)?);

        Ok(HooksCollector {
            dirs: dirs,
            state: state,
            base: base.as_ref().to_path_buf(),
            recursive: recursive,
        })
    }

    fn collect_file(&mut self, e: PathBuf) -> Result<Option<Arc<Hook>>> {
        if e.is_dir() {
            if self.recursive {
                self.dirs.push_back(read_dir(&e)?);
            }
            return Ok(None);
        }

        // Check if the file is executable and readable
        let mode = e.metadata()?.permissions().mode();
        if ! ((mode & 0o111) != 0 && (mode & 0o444) != 0) {
            // Skip files with wrong permissions
            return Ok(None);
        }

        // Try to remove the prefix from the path
        let name = match e.strip_prefix(&self.base) {
            Ok(stripped) => stripped,
            Err(_) => &e,
        }.to_str().unwrap().to_string();

        let exec = canonicalize(&e)?.to_str().unwrap().into();

        Ok(Some(Arc::new(Hook::load(name, exec, &self.state)?)))
    }
}

impl Iterator for HooksCollector {
    type Item = Result<Arc<Hook>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = if let Some(iter) = self.dirs.get_mut(0) {
                iter.next()
            } else {
                // No more directories to search in
                return None;
            };

            match entry {
                // Found an entry
                Some(Ok(entry)) => {
                    match self.collect_file(entry.path()) {
                        Ok(result) => {
                            if let Some(hook) = result {
                                return Some(Ok(hook));
                            }
                            // If None is returned get another one
                        },
                        Err(err) => {
                            return Some(Err(err));
                        },
                    }
                },
                // I/O error while getting the next entry
                Some(Err(err)) => {
                    return Some(Err(err.into()));
                },
                // No more entries in the directory
                None => {
                    // Don't search in this directory anymore
                    let _ = self.dirs.pop_front();
                },
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;
    use std::fs;
    use std::sync::Arc;

    use common::state::State;

    use utils::testing::*;
    use utils;
    use common::prelude::*;
    use providers::StatusEventKind;
    use requests::{Request, RequestType};

    use super::{Hook, HooksCollector, HooksBlueprint};


    macro_rules! assert_hook {
        ($state:expr, $base:expr, $name:expr) => {{
            // Get the hook path
            let mut path = $base.clone();
            path.push($name);
            let path_str = path.to_str().unwrap().to_string();

            let hook = Hook::load(
                $name.to_string(), path_str.clone(), $state,
            ).unwrap();

            assert_eq!(hook.name(), $name.to_string());
            assert_eq!(hook.exec(), path_str.clone());

            Arc::new(hook)
        }};
        ($base:expr, $name:expr) => {{
            assert_hook!(&Arc::new(State::new()), $base, $name)
        }};
    }


    #[test]
    fn test_hooks_status_hooks_collection() {
        let base = utils::create_temp_dir().unwrap();

        // Create a standard hook
        create_hook!(base, "test.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "hi";"#
        );

        // Create two different status hooks
        create_hook!(base, "status1.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Status: {"events": ["job_completed", "job_failed"]}"#,
            r#"echo "hi";"#
        );
        create_hook!(base, "status2.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Status: {"events": ["job_failed"]}"#,
            r#"echo "hi";"#
        );

        let mut blueprint = HooksBlueprint::new(Arc::new(State::new()));
        blueprint.collect_path(&base, false).unwrap();

        let hooks = blueprint.hooks();

        assert_eq!(
            hooks.inner.read().unwrap().status_hooks
                 .get(&StatusEventKind::JobCompleted).unwrap().iter()
                 .map(|hp| hp.hook.name().to_string())
                 .collect::<Vec<String>>(),
            vec!["status1.sh".to_string()]
        );
        assert_eq!(
            {
                let mut status = hooks.inner.read().unwrap().status_hooks
                                      .get(&StatusEventKind::JobFailed)
                                      .unwrap().iter()
                                      .map(|hp| hp.hook.name().to_string())
                                      .collect::<Vec<String>>();
                status.sort();
                status
            },
            vec!["status1.sh".to_string(), "status2.sh".to_string()]
        );

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn test_hooks_blueprint() {
        let base = utils::create_temp_dir().unwrap();
        let other = utils::create_temp_dir().unwrap();

        create_hook!(base, "a.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "a";"#
        );

        create_hook!(base, "b.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "b";"#
        );

        create_hook!(other, "c.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "c";"#
        );

        let mut blueprint = HooksBlueprint::new(Arc::new(State::new()));
        blueprint.insert(assert_hook!(other, "c.sh")).unwrap();
        blueprint.collect_path(&base, false).unwrap();

        let hooks = blueprint.hooks();

        // Check if all the hooks were loaded
        let mut names = hooks.names().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec![
            "a.sh".to_string(),
            "b.sh".to_string(),
            "c.sh".to_string(),
        ]);

        // Update the hooks in the directory
        fs::remove_file(&base.join("b.sh")).unwrap();
        create_hook!(base, "d.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "d";"#
        );

        // Do a reload of the hooks
        blueprint.reload().unwrap();

        // Check if changes were applied
        let mut names = hooks.names().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec![
            "a.sh".to_string(),
            "c.sh".to_string(),
            "d.sh".to_string(),
        ]);

        // Do an invalid reload
        fs::remove_dir_all(&base).unwrap();
        assert!(blueprint.reload().is_err());

        // Ensure no changes were applied
        let mut names = hooks.names().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec![
            "a.sh".to_string(),
            "c.sh".to_string(),
            "d.sh".to_string(),
        ]);

        fs::remove_dir_all(&other).unwrap();
    }

    #[test]
    fn test_collect() {
        let base = utils::create_temp_dir().unwrap();
        let state = Arc::new(State::new());

        // Create two valid hooks
        create_hook!(base, "test-hook.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "hi";"#
        );
        create_hook!(base, "another-test.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "bye";"#
        );

        // Create a directory
        let mut dir_path = base.clone();
        dir_path.push("a-directory");
        fs::create_dir(&dir_path).unwrap();;

        // Create an hook into that directory
        create_hook!(dir_path, "hook-in-subdir.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "I'm useless :(";"#
        );

        // Create a non-executable file
        let mut hook_path = base.clone();
        hook_path.push("non-executable.sh");
        let mut hook = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o644)
            .open(&hook_path)
            .unwrap();
        let res = write!(hook, "{}", concat!(
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: something"#,
            r#"echo "I'm also useless :(("#
        ));
        res.unwrap();

        // Collect all the hooks in the base
        let mut hooks = Vec::new();
        for hook in HooksCollector::new(&base, state.clone(), false).unwrap() {
            hooks.push(hook.unwrap().name().to_string());
        }

        // There should be only two collected hooks
        assert_eq!(hooks.len(), 2);
        assert!(hooks.contains(&"test-hook.sh".to_string()));
        assert!(hooks.contains(&"another-test.sh".to_string()));

        // Collect with recursion
        let mut hooks = Vec::new();
        for hook in HooksCollector::new(&base, state.clone(), true).unwrap() {
            hooks.push(hook.unwrap().name().to_string());
        }

        // There should be only two collected hooks
        assert_eq!(hooks.len(), 3);
        assert!(hooks.contains(&"test-hook.sh".to_string()));
        assert!(hooks.contains(&"another-test.sh".to_string()));
        assert!(hooks.contains(&"a-directory/hook-in-subdir.sh".to_string()));

        // Then add an hook with an invalid provider
        create_hook!(base, "invalid.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-InvalidHookDoNotUseThisNamePlease: invalid"#,
            r#"echo "hi";"#
        );

        // The collection should fail
        let mut error = None;
        for hook in HooksCollector::new(&base, state.clone(), false).unwrap() {
            if let Err(err) = hook {
                error = Some(err);
                break;
            }
        }
        let error = error.unwrap();

        if let ErrorKind::ProviderNotFound(ref name) = *error.kind() {
            assert_eq!(name, "InvalidHookDoNotUseThisNamePlease");
        } else {
            panic!("Wrong error kind: {:?}", error.kind());
        }

        fs::remove_dir_all(&base).unwrap();
    }


    #[test]
    fn test_validate() {
        let state = Arc::new(State::new());
        let base = utils::create_temp_dir().unwrap();

        create_hook!(base, "single.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: {}"#,
            r#"echo "ok""#
        );

        create_hook!(base, "failing.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Standalone: {"secret": "hi"}"#,
            r#"echo "ok""#
        );

        create_hook!(base, "multiple1.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Testing: {}"#,
            r#"## Fisher-Standalone: {"secret": "hi"}"#
        );

        create_hook!(base, "multiple2.sh",
            r#"#!/bin/bash"#,
            r#"## Fisher-Standalone: {"secret": "hi"}"#,
            r#"## Fisher-Testing: {}"#
        );

        let single = assert_hook!(&state, base, "single.sh");
        let failing = assert_hook!(&state, base, "failing.sh");
        let multiple1 = assert_hook!(&state, base, "multiple1.sh");
        let multiple2 = assert_hook!(&state, base, "multiple2.sh");

        let req = Request::Web(dummy_web_request());

        assert!(single.validate(&req).0 == RequestType::ExecuteHook);
        assert!(failing.validate(&req).0 == RequestType::Invalid);
        assert!(multiple1.validate(&req).0 == RequestType::ExecuteHook);
        assert!(multiple2.validate(&req).0 == RequestType::ExecuteHook);

        fs::remove_dir_all(&base).unwrap();
    }
}
