//! BLE helper for Phomemo CUPS backend.
//!
//! Scans for Phomemo BLE printers and sends ESC/POS data via GATT writes.
//! Called by the `phomemo-ble` CUPS backend wrapper which runs this binary
//! in the logged-in user's context for Bluetooth TCC permission.

use std::io::{self, Read};
use std::process;
use std::time::Duration;

use btleplug::api::{Central, CharPropFlags, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::Manager;
use futures::StreamExt;
use tokio::time;
use uuid::Uuid;

/// Phomemo BLE write characteristic UUID (short: FF02).
const WRITE_CHAR_UUID: Uuid = Uuid::from_u128(0x0000ff02_0000_1000_8000_00805f9b34fb);

/// Phomemo BLE notify characteristic (FF03). The M220 ignores a job unless
/// the host has subscribed to it first (verified on a real M220, 2026-11-16).
const NOTIFY_CHAR_UUID: Uuid = Uuid::from_u128(0x0000ff03_0000_1000_8000_00805f9b34fb);

/// How long to wait for the printer's end-of-job status (first byte 0x1A).
const DONE_TIMEOUT: Duration = Duration::from_secs(30);

const SCAN_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_SIZE: usize = 100;

/// Send this many chunks with WriteWithoutResponse (fast burst), then
/// one chunk with WriteWithResponse as a sync barrier.  The sync write
/// blocks until the printer ACKs, which it does only when its receive
/// buffer has space.  This keeps the buffer full (no thermal head
/// starvation) while preventing overflow (no banding artefacts).
const SYNC_INTERVAL: usize = 8; // sync every ~800 bytes

/// Fallback delay when the characteristic lacks WRITE support (only
/// WRITE_WITHOUT_RESPONSE available — no sync barrier possible).
const FALLBACK_DELAY: Duration = Duration::from_millis(50);
const DRAIN_DELAY: Duration = Duration::from_secs(1);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

/// Discovery mode: scan for Phomemo BLE printers and output CUPS device lines.
async fn discover() -> Result<(), Box<dyn std::error::Error>> {
    let manager = Manager::new().await?;
    let central = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("no Bluetooth adapter found")?;

    central.start_scan(ScanFilter::default()).await?;
    time::sleep(SCAN_TIMEOUT).await;
    central.stop_scan().await?;

    for peripheral in central.peripherals().await? {
        if let Some(props) = peripheral.properties().await? {
            if let Some(name) = &props.local_name {
                let is_phomemo = name.starts_with("M110")
                    || name.starts_with("M120")
                    || name.starts_with("M220")
                    || name.starts_with("M02")
                    || name.starts_with("D30")
                    || name.starts_with("T02")
                    || name.starts_with("Q0");

                if is_phomemo {
                    println!(
                        "direct phomemo-ble://{name} \
                         \"Phomemo BLE ({name})\" \
                         \"Phomemo Label Printer BLE ({name})\" \
                         \"MFG:Phomemo;CMD:ESCPOS;\""
                    );
                }
            }
        }
    }

    Ok(())
}

/// Send print data to a named Phomemo printer via BLE GATT writes.
async fn send_data(device_name: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let manager = Manager::new().await?;
    let central = manager
        .adapters()
        .await?
        .into_iter()
        .next()
        .ok_or("no Bluetooth adapter found")?;

    central.start_scan(ScanFilter::default()).await?;

    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    let peripheral = loop {
        if tokio::time::Instant::now() > deadline {
            central.stop_scan().await?;
            return Err(format!("printer '{device_name}' not found").into());
        }

        time::sleep(Duration::from_millis(200)).await;

        let mut found = None;
        for p in central.peripherals().await? {
            if let Some(props) = p.properties().await? {
                if props.local_name.as_deref() == Some(device_name) {
                    found = Some(p);
                    break;
                }
            }
        }

        if let Some(p) = found {
            central.stop_scan().await?;
            break p;
        }
    };

    peripheral.connect().await?;
    peripheral.discover_services().await?;

    // Subscribe to FF03 before writing anything, then give the printer a
    // moment. Without this the M220 accepts the bytes and prints nothing.
    let notify_char = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == NOTIFY_CHAR_UUID);
    let mut notifications = None;
    if let Some(nc) = &notify_char {
        peripheral.subscribe(nc).await?;
        notifications = Some(peripheral.notifications().await?);
        time::sleep(Duration::from_secs(1)).await;
    }

    let write_char = peripheral
        .characteristics()
        .into_iter()
        .find(|c| c.uuid == WRITE_CHAR_UUID)
        .or_else(|| {
            peripheral
                .characteristics()
                .into_iter()
                .find(|c| c.properties.contains(CharPropFlags::WRITE_WITHOUT_RESPONSE))
        })
        .ok_or("no writable characteristic found on printer")?;

    let can_sync = write_char.properties.contains(CharPropFlags::WRITE);

    // Hybrid flow control: burst fast with WriteWithoutResponse to keep
    // the printer's buffer full, then periodically send one chunk with
    // WriteWithResponse as a sync barrier.  The sync write blocks until
    // the printer ACKs, preventing buffer overflow without starving the
    // thermal head between every single chunk.
    for (i, chunk) in data.chunks(CHUNK_SIZE).enumerate() {
        let is_sync = can_sync && i > 0 && i % SYNC_INTERVAL == 0;

        if is_sync {
            peripheral
                .write(&write_char, chunk, WriteType::WithResponse)
                .await?;
        } else {
            peripheral
                .write(&write_char, chunk, WriteType::WithoutResponse)
                .await?;
            if !can_sync {
                time::sleep(FALLBACK_DELAY).await;
            }
        }
    }

    // Wait for the printer's end-of-job status (0x1A ...) before hanging up,
    // so disconnecting cannot cut the print short.
    if let Some(stream) = notifications.as_mut() {
        let _ = time::timeout(DONE_TIMEOUT, async {
            while let Some(n) = stream.next().await {
                if n.value.first() == Some(&0x1a) {
                    break;
                }
            }
        })
        .await;
    }

    time::sleep(DRAIN_DELAY).await;
    peripheral.disconnect().await?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let result = time::timeout(PROCESS_TIMEOUT, async {
        if args.len() <= 1 {
            if let Err(e) = discover().await {
                eprintln!("ERROR: phomemo-ble: {e}");
            }
            return;
        }

        let device_uri = std::env::var("DEVICE_URI").unwrap_or_default();
        if device_uri.is_empty() {
            eprintln!("ERROR: No DEVICE_URI set");
            process::exit(1);
        }

        let device_name = device_uri
            .strip_prefix("phomemo-ble://")
            .unwrap_or(&device_uri);

        let mut data = Vec::new();
        if args.len() >= 7 {
            std::fs::File::open(&args[6])
                .and_then(|mut f| f.read_to_end(&mut data))
                .unwrap_or_else(|e| {
                    eprintln!("ERROR: cannot read input file: {e}");
                    process::exit(1);
                });
        } else {
            io::stdin().read_to_end(&mut data).unwrap_or_else(|e| {
                eprintln!("ERROR: cannot read stdin: {e}");
                process::exit(1);
            });
        }

        if data.is_empty() {
            eprintln!("ERROR: no data to send");
            process::exit(1);
        }

        if let Err(e) = send_data(device_name, &data).await {
            eprintln!("ERROR: phomemo-ble: {e}");
            process::exit(1);
        }
    })
    .await;

    if result.is_err() {
        eprintln!("ERROR: phomemo-ble: timed out after {PROCESS_TIMEOUT:?}");
        process::exit(1);
    }
}
