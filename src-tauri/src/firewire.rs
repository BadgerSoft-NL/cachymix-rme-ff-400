//! Asynchrone FireWire-transacties via de character device van de kernel
//! (`/dev/fwN`, zie `linux/firewire-cdev.h`).
//!
//! Dit is precies wat hinawa onder snd-firewire-ctl-services ook doet, alleen
//! zonder de GLib eromheen: een ioctl om het verzoek te sturen en een `read()`
//! om het antwoord op te halen. De node waar het bestand voor staat is het
//! doel; een adres is dus alleen nog de 48-bit offset.

use std::fs::{File, OpenOptions};
use std::io;
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

const EVENT_BUS_RESET: u32 = 0x00;
const EVENT_RESPONSE: u32 = 0x01;

// Vóór versie 6 komt een antwoord als FW_CDEV_EVENT_RESPONSE zonder tijdstempels.
const ABI_VERSIE: u32 = 4;

pub const TCODE_WRITE_QUADLET: u32 = 0x0;
pub const TCODE_WRITE_BLOCK: u32 = 0x1;
pub const TCODE_READ_BLOCK: u32 = 0x5;

const RCODE_COMPLETE: u32 = 0x00;
const RCODE_BUSY: u32 = 0x12;
const RCODE_GENERATION: u32 = 0x13;

#[repr(C)]
#[derive(Default)]
struct GetInfo {
    version: u32,
    rom_length: u32,
    rom: u64,
    bus_reset: u64,
    bus_reset_closure: u64,
    card: u32,
}

#[repr(C)]
#[derive(Default)]
struct SendRequest {
    tcode: u32,
    length: u32,
    offset: u64,
    closure: u64,
    data: u64,
    generation: u32,
}

#[repr(C)]
#[derive(Default)]
struct BusReset {
    closure: u64,
    kind: u32,
    node_id: u32,
    local_node_id: u32,
    bm_node_id: u32,
    irm_node_id: u32,
    root_node_id: u32,
    generation: u32,
}

const fn ioc(dir: u64, nr: u64, size: usize) -> u64 {
    (dir << 30) | ((size as u64) << 16) | ((b'#' as u64) << 8) | nr
}

const IOC_GET_INFO: u64 = ioc(3, 0x00, size_of::<GetInfo>());
const IOC_SEND_REQUEST: u64 = ioc(1, 0x01, size_of::<SendRequest>());

pub struct Node {
    bestand: File,
    generatie: u32,
    volgnummer: u64,
}

impl Node {
    pub fn open(pad: &Path) -> io::Result<Self> {
        let bestand = OpenOptions::new().read(true).write(true).open(pad)?;
        let mut node = Node {
            bestand,
            generatie: 0,
            volgnummer: 1,
        };
        node.ververs_generatie()?;
        Ok(node)
    }

    /// Haalt de huidige busgeneratie op. Na een bus reset weigert de kernel
    /// verzoeken met de oude, dus dit is ook de weg terug na zo'n fout.
    fn ververs_generatie(&mut self) -> io::Result<()> {
        let mut reset = BusReset::default();
        let mut info = GetInfo {
            version: ABI_VERSIE,
            bus_reset: &mut reset as *mut BusReset as u64,
            ..Default::default()
        };
        let r = unsafe {
            libc::ioctl(
                self.bestand.as_raw_fd(),
                IOC_GET_INFO as _,
                &mut info as *mut GetInfo,
            )
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        self.generatie = reset.generation;
        Ok(())
    }

    /// Eén transactie. Bij schrijven gaat `data` de draad op, bij lezen komt
    /// het antwoord erin terug. Een bezette node en een bus reset tussendoor
    /// worden een paar keer opnieuw geprobeerd; al het andere is een fout.
    pub fn transactie(&mut self, tcode: u32, offset: u64, data: &mut [u8]) -> io::Result<()> {
        let mut pogingen = 0;
        loop {
            pogingen += 1;
            match self.probeer(tcode, offset, data) {
                Ok(RCODE_COMPLETE) => return Ok(()),
                Ok(rcode @ (RCODE_BUSY | RCODE_GENERATION)) if pogingen < 5 => {
                    if rcode == RCODE_GENERATION {
                        self.ververs_generatie()?;
                    } else {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                }
                Ok(rcode) => {
                    return Err(io::Error::other(format!(
                        "transactie op {offset:#014x} mislukt (rcode {rcode:#x})"
                    )))
                }
                Err(e) if e.raw_os_error() == Some(libc::ESTALE) && pogingen < 5 => {
                    self.ververs_generatie()?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn probeer(&mut self, tcode: u32, offset: u64, data: &mut [u8]) -> io::Result<u32> {
        let closure = self.volgnummer;
        self.volgnummer += 1;

        let mut verzoek = SendRequest {
            tcode,
            length: data.len() as u32,
            offset,
            closure,
            data: data.as_mut_ptr() as u64,
            generation: self.generatie,
        };
        let r = unsafe {
            libc::ioctl(
                self.bestand.as_raw_fd(),
                IOC_SEND_REQUEST as _,
                &mut verzoek as *mut SendRequest,
            )
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }

        let lezend = tcode == TCODE_READ_BLOCK;
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut buf = [0u8; 4096];
        loop {
            let over = deadline.saturating_duration_since(Instant::now());
            if over.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "geen antwoord"));
            }
            let mut pfd = libc::pollfd {
                fd: self.bestand.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let r = unsafe { libc::poll(&mut pfd, 1, over.as_millis() as i32 + 1) };
            if r < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if r == 0 {
                continue;
            }
            if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::NotConnected,
                    "apparaat verdwenen",
                ));
            }
            let n = unsafe {
                libc::read(
                    self.bestand.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                )
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            let n = n as usize;
            if n < 12 {
                continue;
            }
            let woord = |i: usize| u32::from_ne_bytes(buf[i..i + 4].try_into().unwrap());
            let evt_closure = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
            match woord(8) {
                EVENT_BUS_RESET if n >= 36 => {
                    // generation staat als laatste veld in fw_cdev_event_bus_reset
                    self.generatie = woord(32);
                }
                EVENT_RESPONSE if evt_closure == closure && n >= 20 => {
                    let rcode = woord(12);
                    let lengte = woord(16) as usize;
                    if lezend && rcode == RCODE_COMPLETE {
                        let lengte = lengte.min(data.len()).min(n - 20);
                        data[..lengte].copy_from_slice(&buf[20..20 + lengte]);
                    }
                    return Ok(rcode);
                }
                _ => {}
            }
        }
    }

    pub fn schrijf_quadlet(&mut self, offset: u64, waarde: u32) -> io::Result<()> {
        // RME verwacht de quadlets little-endian op de draad, anders dan de
        // rest van IEEE 1394.
        let mut raw = waarde.to_le_bytes();
        self.transactie(TCODE_WRITE_QUADLET, offset, &mut raw)
    }

    pub fn schrijf_blok(&mut self, offset: u64, quadlets: &[u32]) -> io::Result<()> {
        let mut raw: Vec<u8> = quadlets.iter().flat_map(|q| q.to_le_bytes()).collect();
        self.transactie(TCODE_WRITE_BLOCK, offset, &mut raw)
    }

    pub fn lees_blok(&mut self, offset: u64, aantal: usize) -> io::Result<Vec<u32>> {
        let mut raw = vec![0u8; aantal * 4];
        self.transactie(TCODE_READ_BLOCK, offset, &mut raw)?;
        Ok(raw
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn ioctl_nummers_kloppen_met_de_header() {
        // _IOWR('#', 0x00, struct fw_cdev_get_info) en _IOW('#', 0x01, ...)
        assert_eq!(size_of::<GetInfo>(), 40);
        assert_eq!(size_of::<SendRequest>(), 40);
        assert_eq!(IOC_GET_INFO, 0xc028_2300);
        assert_eq!(IOC_SEND_REQUEST, 0x4028_2301);
        assert_eq!(size_of::<BusReset>(), 40);
    }
}
