# CLI

After `pip install -e .`, use the `copaw` command.

## Commands

| Command                 | Description                                  |
| ----------------------- | -------------------------------------------- |
| `copaw init`            | Interactive setup: config.json, HEARTBEAT.md |
| `copaw init --defaults` | Non-interactive, use defaults                |
| `copaw app`             | Start FastAPI server (default :8088)         |
| `copaw cron list`       | List cron jobs (requires app running)        |
| `copaw clean`           | Wipe working directory (with confirmation)   |

See project README for full reference.
