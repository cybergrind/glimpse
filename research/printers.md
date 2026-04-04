# Printers Provider

**Source:** CUPS (Common Unix Printing System) via HTTP API or CLI

**What it does:** Lists printers, reports print queue status, and manages print jobs.

## System Interface

### CUPS HTTP API

CUPS exposes an IPP (Internet Printing Protocol) server at `localhost:631`.

Key operations via `lpstat` / `lp` CLI:
- `lpstat -p -d` — list printers and default
- `lpstat -o` — list pending jobs
- `lp -d PRINTER FILE` — print a file
- `cancel JOB_ID` — cancel a print job
- `lpadmin -d PRINTER` — set default printer
- `lpstat -l -p PRINTER` — detailed printer info

### CUPS D-Bus notifications (optional)

Service: `org.cups.cupsd.Notifier` (system bus)

Signals (if CUPS D-Bus support is enabled):
- `JobCreated`, `JobCompleted`, `JobStopped`, `JobProgress`
- `PrinterAdded`, `PrinterRemoved`, `PrinterStateChanged`

Note: CUPS D-Bus notifications are optional and may not be enabled on all systems.

## Topics

- `printers.list` — all printers with status
- `printers.queue` — active print jobs
- `printers.printer.{name}` — single printer status

## Methods

- `printers.set_default(printer: String)` — set default printer
- `printers.cancel_job(job_id: u32)` — cancel a print job
- `printers.enable(printer: String)` — enable a printer
- `printers.disable(printer: String)` — disable a printer

## Types

```rust
/// Printer state
enum PrinterState {
    Idle,
    Processing,
    Stopped,
}

/// A printer
struct Printer {
    name: String,
    description: String,
    location: String,
    state: PrinterState,
    state_message: String,
    is_default: bool,
    is_accepting_jobs: bool,
    /// Whether printer is shared on network
    is_shared: bool,
}

/// A print job
struct PrintJob {
    id: u32,
    printer: String,
    title: String,
    user: String,
    state: PrintJobState,
    /// Size in bytes
    size: u64,
    /// Pages printed / total
    pages_completed: u32,
    pages_total: u32,
    /// Submission time (Unix timestamp)
    created: u64,
}

enum PrintJobState {
    Pending,
    Processing,
    Held,
    Completed,
    Cancelled,
    Aborted,
}
```

## Icons

- `printer-symbolic` — printer device
- `printer-error-symbolic` — printer error
- `printer-printing-symbolic` — printing in progress (if available)

All icons above are available in Adwaita icon theme.

## Crates

- `cups` or `ipp` — CUPS/IPP client (if available)
- Alternative: parse `lpstat` CLI output

## Change Detection

**CUPS D-Bus signals (if available):** `PrinterStateChanged`, `JobCreated`, `JobCompleted`. Reactive but not guaranteed on all systems.

**Polling fallback:** Run `lpstat -o` every 5–10 seconds to check job queue.

## Features

- List all printers with status (idle, processing, stopped)
- Default printer detection and switching
- Print queue listing with job progress
- Cancel print jobs
- Enable/disable printers
- Printer location and description
- Network printer detection
- Job state tracking (pending, processing, completed, cancelled)

## Notes

- CUPS is not installed on all systems — provider should handle absence
- CUPS D-Bus notification support varies — don't rely on it, have polling fallback
- IPP protocol is HTTP-based — could use reqwest for direct API access
- Print operations may require polkit authentication
- This is a lower-priority provider — many desktop users don't print
