#!/bin/bash

function get_project_regex() {
    regex="${1#|}"
    regex="${regex%|}"
    regex="${regex//:/.}"
    regex="${regex//-/.}"
    printf %s "$regex"
}

if [ -f ./abt ] ; then
    CMD=./abt
else
    CMD=abt
fi

projects=`get_project_regex $("$CMD" "$@" --dry-run -q 2>&1 | grep -Eo 'Add project \S+' | while read line ; do
    printf %s "|^${line#Add project }$"
done)`
echo projects $projects

extra_projects=`get_project_regex $("$CMD" -e "$projects" -f 'grep -E "project\s*\(" build.gradle*' -- -v 2>&1 | grep -E 'project\s*\(' |  while read line ; do
    if [[ "$line" =~ project[^:]+:([^\"\']+) ]] ; then
        printf %s "|^${BASH_REMATCH[1]}$"
    fi
done)`
echo $extra_projects

"$CMD" -e "$extra_projects" -f 'touch XXX' -- -v 1>/dev/null 2>&1
if [[ "$*" == *" -s "* ]] ; then
    "$CMD" "$@"
else
    "$CMD" -s HEAD "$@"
fi
"$CMD" -e "$extra_projects" -f 'touch XXX' -- -v 1>/dev/null 2>&1
