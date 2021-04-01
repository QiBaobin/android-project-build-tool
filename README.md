# android-project-build-tool

## install

* cargo

``` sh
cargo install --git https://github.com/QiBaobin/android-project-build-tool.git
```

* download from release and make it excutable

## usage

``` sh
the android build tools

USAGE:
    abt [FLAGS] [OPTIONS] <SUBCOMMAND>

FLAGS:
    -c, --contain-local-references    if projects contains local references, if so we can't build module separately
    -h, --help                        Prints help information
    -V, --version                     Prints version information
    -v                                verbose, can be provided many times

OPTIONS:
    -e, --excluded-projects <excluded-projects>
            the regex of projects' name under root_project_dir we want to exclude always [default: module-
            templates|build-tools|root-project.*|buildSrc|^$]
    -g, --gradle-cmd <gradle-cmd>
            the gradel command to run for building, you can give args here too [env: GRADLE_CMD=]

    -t, --templates-dir <templates-dir>            the dirtory to contain android and domain templates

SUBCOMMANDS:
    build           build modules
    create          Create new features
    help            Prints this message or the help of the given subcommand(s)
    open            Control what modules will be included in default project
    pull-request    Create a pull request
    users
```
