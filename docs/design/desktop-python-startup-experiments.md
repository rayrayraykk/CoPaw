# Desktop Python Startup Experiments

## Objective

Measure and reduce the packaged desktop backend time from process creation to
the default agent becoming ready. The experiments compare three independent
approaches before any larger desktop architecture change:

1. Reduce the Python import graph while retaining PyInstaller onedir.
2. Compile the unchanged application with Nuitka standalone.
3. Run the unchanged application with an embedded CPython distribution.

The target is `process start -> default_agent_ready` at or below 1000 ms. A
port bind or HTTP health response alone does not satisfy the target.

## Branch topology

All experiment branches share one benchmark commit based on the same
`upstream/main` revision:

```text
upstream/main
  `-- perf/desktop-python-benchmark
        |-- perf/desktop-python-imports
        |-- experiment/desktop-nuitka
        `-- experiment/desktop-embedded-python
```

The packaging experiments do not include the import changes initially. This
keeps attribution clear. A final branch may combine the import changes with
the best packaging result after the independent measurements complete.

## Milestones

The Python sidecar emits newline-delimited JSON records when
`QWENPAW_DESKTOP_STARTUP_METRICS=1`:

| Event | Meaning |
| --- | --- |
| `python_entry` | The frozen Python entry module began executing. |
| `desktop_runtime_installed` | Desktop process/runtime guards are installed. |
| `sidecar_logging_installed` | Persistent sidecar logging is ready. |
| `backend_server_imports_started` | Server-specific imports begin. |
| `fastapi_app_import_started` | The complete FastAPI application import begins. |
| `fastapi_app_import_done` | The complete FastAPI application import ends. |
| `port_bound` | The backend has bound its loopback socket. |
| `lifespan_started` | Uvicorn entered the FastAPI lifespan. |
| `core_ready` | Synchronous lifespan work completed. |
| `default_agent_ready` | The core configured-agent phase completed. |

The benchmark records both the child process monotonic duration and the
duration reported inside Python. Their difference approximates process,
bootloader, and pipe-delivery overhead.

## Measurement profiles

CI runs one unmeasured warmup against a temporary working directory, then ten
measured process launches against the initialized directory. Each launch is a
new backend process. This isolates steady application startup from first-user
configuration creation while retaining a fresh GitHub-hosted runner for each
job.

The exploratory branch runs use ten samples to control CI cost. The winning
configuration must later run at least thirty samples before a performance
claim is made.

Reported fields:

- every structured milestone;
- external process-to-target wall time;
- P50, P90, and P95;
- individual run failures and output tails;
- executable path and platform.

GitHub-hosted runners provide useful relative comparisons, not a consumer
hardware guarantee. A final Windows target must also be verified on a stable
reference machine with Microsoft Defender enabled.

## Experiment boundaries

### Import graph

The import experiment may change import boundaries and registration data but
must not change user-visible capabilities or introduce a second compatibility
path. It retains the current PyInstaller onedir packaging.

Primary candidates identified by the initial profile are local-model imports,
provider SDK imports, provider catalog construction, router imports, browser
runtime imports, and package-level re-exports.

### Nuitka

The Nuitka experiment changes packaging only. It must preserve dynamic plugin
imports, subprocess behavior, bundled Python behavior for plugin dependencies,
certificates, package data, and the Tauri sidecar contract.

### Embedded CPython

The embedded experiment changes packaging only. Pure Python bytecode may live
in an application archive. Native extensions remain filesystem resources. It
must preserve `sys.executable`, multiprocessing, plugin dependency paths, and
the Tauri sidecar contract.

## Local and CI responsibilities

Local work is limited to source changes, import profiling, unit tests, type
checks, and Rust checks. Formal PyInstaller, Nuitka, embedded Python, Tauri,
installer, and launch verification runs happen in GitHub Actions.

Experiment branches are pushed only to `origin`. The work does not push to
`upstream`, publish a release, upload to the production object store, or create
a pull request without separate approval.

## Checklist

- [x] Create the benchmark branch from current `upstream/main`.
- [x] Define structured startup milestones.
- [x] Add a cross-platform packaged-backend benchmark runner.
- [x] Add unit tests for metrics and benchmark aggregation.
- [x] Add Windows and macOS benchmark artifact collection to CI.
- [x] Run local lightweight validation.
- [ ] Push the benchmark branch to `origin`.
- [ ] Run the PyInstaller baseline workflow.
- [ ] Analyze and record baseline artifacts.
- [ ] Create and run the import experiment.
- [ ] Create and run the Nuitka experiment.
- [ ] Create and run the embedded CPython experiment.
- [ ] Compare all independent results.
- [ ] Combine the winning packaging approach with import optimization.
