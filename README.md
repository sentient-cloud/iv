# iv

i got tired of the gnome image viewer not opening webp's

## Usage:

`iv [args..] [dir]` open directory and view files in dir, dir defaults to `$PWD`, dir must be a directory

### Arguments

| Short | Long         | Purpose                                      | Defaults to                                  | Value type |
| ----- | ------------ | -------------------------------------------- | -------------------------------------------- | ---------- |
|       | `--help`     | Print help and exit                          |                                              |            |
| `-H`  | `--host`     | Host                                         | `127.0.0.1`                                  | ipv4/6     |
| `-p`  | `--port`     | Port                                         | `8888`                                       | u16        |
| `-n`  | `--no-open`  | Do not open in default browser automatically | `if cfg!(debug_assertions) then off else on` | flag       |
| `-t`  | `--traverse` | Allow directory traversal                    | `off`                                        | flag       |
| `-v`  | `--verbose`  | Verbose level log output                     | `off`                                        | flag       |
|       | `--trace`    | Trace level log output                       | `off`                                        | flag       |
