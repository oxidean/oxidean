# oxl

`oxl mod init` style module — package `main` with a `test_*` file.

## Getting started

```bash
oxl build
oxl run
oxl test
```

Requires the `oxl` toolchain on PATH plus a C compiler (`cc`) and
pthreads — `oxl build`/`run` emit C and invoke `cc` for the final
link. See https://github.com/oxidean/oxl.
