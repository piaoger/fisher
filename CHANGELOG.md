# Fisher changelog

This page contains the full list of the changes in each Fisher release.
Internal changes without any visible effect aren't documented, but you can find
everything a user can notice.

## Fisher 1.0.x

### Next release

* **New features:**

    * [Configuration files](docs/config.md) are now supported
    * The [Standalone provider](providers/standalone.md) now supports
      whitelisting IP addresses
    * [Rate limits](features/rate-limits.md) are now supported for invalid
      requests

* **Changes and improvements:**

    * **BREAKING:** the `SHELL` environment variable is not present anymore
    * **BREAKING:** most of the CLI arguments are now removed
    * The `USER` environment variable is now guaranteed to be correct

### Fisher 1.0.0-beta.7

*Released on August 16th, 2017.*

* Fix hooks with multiple providers not validated properly

### Fisher 1.0.0-beta.6

*Released on May 10th, 2017.*

* Update dependencies versions

### Fisher 1.0.0-beta.5

*Released on April 23th, 2017.*

* **New features:**

    * Add support for reloading the hooks on the fly with the SIGUSR1 signal

### Fisher 1.0.0-beta.4

*Released on April 10th, 2017.*

* **New features:**

    * Add the `max_threads` field to `GET /health`
    * Add the `label`, `milestone`, `organization`, `project_card`,
     `project_column`, `project`, `pull_request_review`, `team` GitHub events
    * Add the ability to provide extra environment variables with the `-e` flag
    * Add the ability to load hooks in subdirectories with the `-r` flag
    * Add the ability to set priorities for hooks
    * Add the ability to disable parallel execution for certain hooks

* **Changes and improvements:**

    * **BREAKING:** `$FISHER_REQUEST_BODY` is not available anymore on status
      hooks
    * **BREAKING:** Rename `queue_size` to `queued_jobs` in `GET /health` for
      consistency
    * **BREAKING:** Rename `active_jobs` to `busy_threads` in `GET /health` for
      consistency
    * **BREAKING:** The extension of the files is needed when calling the hooks
      (for example you need to call `/hook/example.sh` instead of `/hook/example`)
    * Speed up status hooks processing
    * Replace the old processor with a faster one
    * Improve testing coverage of the project

* **Bug fixes:**

    * Avoid killing the running jobs when a signal is received
    * Fix GitHub pings not being delivered if a events whitelist was present
    * Fix web server not replying to incoming requests while shutting down

### Fisher 1.0.0-beta.3

*Released on January 5th, 2017.*

* Add the `$FISHER_REQUEST_IP` environment variable
* Add support for status hooks
* Refactored a bunch of the code
* Improve testing coverage of the project

### Fisher 1.0.0-beta.2

*Released on September 24th, 2016.*

* Add support for working behind proxies
* Add support for receiving hooks from **GitLab**
* Show the current Fisher configuration at startup
* Improve unit testing coverage of the project

### Fisher 1.0.0-beta.1

*Released on September 6th, 2016.*

* Initial public release of Fisher
