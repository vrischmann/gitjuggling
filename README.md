# gitjuggling

This is a simple tool to run a git command in all repositories under the current working directory.

For example, with the following directory layout:
```
.
├── bar
│   ├── foobar
│   └── .git
├── baz
│   ├── foobar
│   └── .git
└── foo
    ├── foobar
    └── .git
```

You can run `git pull` in all repositories like this:
```
$ gitjuggling fetch --all -p
/tmp/test/foo executing fetch --all -p
/tmp/test/baz executing fetch --all -p
/tmp/test/bar executing fetch --all -p
3 items succeeded, 0 items failed
```
