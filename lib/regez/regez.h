#include <regex.h>

extern regex_t re;

int compile(char const *pattern) {
  return regcomp(&re, pattern, REG_EXTENDED);
}

int isMatch(char const *input) {
  regmatch_t pmatch[0];
  return regexec(&re, input, 0, pmatch, 0);
}
