# Development

## setup development enviroment if need

``` sh
brew install zig
```

## change code

## publish binary to artifactory

Start docker, this is needed because we also need cross compile linux version

``` sh
./build.sh
```


# Usages


### Commands it supports

``` shell
./abt --help
Usage: abt [options] [--] [gradle command]

Options:

  -s, --since-commit             Only select projects changed since given commit in this repo
  -i, --include                  Include projects under given path
  -e, --regexp                   A project is selected if its name matches given pattern
  -v, --invert-match             A project is NOT selected if its name matches given pattern
  -f, --filter                   A project is selected if the given shell command pass in its directory
  -c, --settings-file            The gradle settings file will be generated and used
  --threshold                    The max number of project can run at one time, projects more than it will be sepearted into many run
  --max-depth                    Descend at most n directory levels
  -h, --help                     Print command-specific usage

Environments:

 GRADLE_CMD                      The gradel command to run for building, you can give args here too
  
```

#### Examples

``` shell
./abt build # build all changed projects comapring with remote branch

./abt -e 'core$' build # build all projects with core as name suffix, even they are not changed

```

