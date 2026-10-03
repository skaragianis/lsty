# Simpletask fixtures

These files were written by hand, not saved by the application. They follow
the save format of Simpletask for Android 11.0.1 (mpcjanssen/simpletask-android,
`versionName` in `app/build.gradle` on master), read from its source:

- tasks are joined with `\n` and the end-of-line is configurable, so
  `crlf.txt` uses `\r\n` as Simpletask does with its CRLF setting
- `due:` and `t:` hold `YYYY-MM-DD`, `rec:` holds `1w` or `+1m` (strict),
  `h:1` hides a task, and completion writes `x <completed> <created> ...`
- `no-trailing-newline.txt` has no final line ending

The application itself was not run.
