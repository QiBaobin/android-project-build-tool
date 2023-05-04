# android-project-build-tool

## install

* cargo

``` sh
zig build -p /usr/
```

## usage

``` sh
Usage: abt [options] [--] [gradle command]

Options:

  -s, --since-commit             Only select projects changed since given commit in this repo
  -i, --include                  Include projects under given path
  -e, --regexp                   A project is selected if its name matches given pattern
  -v, --invert-match             A project is NOT selected if its name matches given pattern
  -c, --settings-file            The gradle settings file will be generated and used
  --threshold                    The max number of project can run at one time, projects more than it will be sepearted into many run
  --max-depth                    Descend at most n directory levels
  --scan-impacted-projects       Add projects impacted by selected projects too
  -h, --help                     Print command-specific usage

Environments:

 GRADLE_CMD                      The gradel command to run for building, you can give args here too

```
