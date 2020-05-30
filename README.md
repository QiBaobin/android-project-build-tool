# android-project-build-tool

## install

* cargo

``` sh
cargo build
```

* download from release and make it excutable

## usage

``` sh
the build tools commands

USAGE:
    build-tools [FLAGS] [OPTIONS] <SUBCOMMAND>

FLAGS:
    -c, --contain-local-references    if projects contains local references, if so we can't build module separately
    -h, --help                        Prints help information
    -V, --version                     Prints version information
    -v                                verbose, can be provided many times

OPTIONS:
    -e, --excluded-projects <excluded-projects>
            the regex of projects' name under root_project_dir we want to exclude always [default: module-
            templates|build-tools|root-project.*]
    -g, --gradle-cmd <gradle-cmd>
            the gradel command to run for building, you can give args here too [env: GRADLE_CMD=]

    -r, --root-project-dir <root-project-dir>
            the dirtory to contain root build.gradle.kts, settings.gradle.kts etc [default: root-project]

    -t, --templates-dir <templates-dir>            the dirtory to contain android and domain templates

SUBCOMMANDS:
    build           build modules
    create          Create new features
    help            Prints this message or the help of the given subcommand(s)
    open            Control what modules will be included in default project
    pull-request    Create a pull request
    users
```
