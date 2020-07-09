extern crate midir;

use std::io::{stdin, stdout, Write, ErrorKind};
use std::error::Error;
use std::result::Result;
use midir::{MidiInput, MidiOutput, MidiIO, Ignore};

#[cfg(not(target_arch="wasm32"))]
fn run() -> Result<(), Box<dyn Error>> {
  let mut midi_in = MidiInput::new("midir forwarding input").expect("midir forwarding input");
  let midi_out = MidiOutput::new("midir forwarding output").expect("midir forwarding output");

  midi_in.ignore(Ignore::None);

  let in_port = select_port(&midi_in, "Traktor Kontrol S3 MIDI").expect("Traktor Kontrol S3");
  let out_port = select_port(&midi_out, "Pioneer DDJ-SX").expect("Virtual DDJ-SX");

  println!("\nOpening connections");
  let in_port_name = midi_in.port_name(&in_port)?;
  let out_port_name = midi_out.port_name(&out_port)?;

  let mut conn_out = midi_out.connect(&out_port, "midir-forward").expect("outgoing connection");

  let _conn_in = midi_in.connect(&in_port, "midir-forward", move |stamp, data, _| {
    let status = data[0];
    let msg = status / 16u8;
    let ch  = status % 16u8;

    // jog
    if msg == 0xB && data.len() == 3 && data[1] == 30u8 {
      let new_ch = (if ch % 2 == 0 {ch / 2u8} else {ch}) / 2u8;
      let new_data : [u8;3] = [
        0xB0u8 + new_ch,
        if ch % 2 == 0 { 0x1Fu8 } else { 0x22u8 },
        data[2]
      ];
      // println!("{}: {:?} => {:?}", stamp, data, new_data);
      conn_out.send(&new_data).unwrap_or_else(|_| println!("Error when forwarding message ..."));
    }
  }, ())?;

  println!("Connections open, forwarding from '{}' to '{}' (press enter to exit) ...", in_port_name, out_port_name);

  let mut input = String::new();
  stdin().read_line(&mut input)?; // wait for next enter key press

  println!("Closing connections");
  Ok(())
}

fn main() {
  match run() {
    Ok(_) => (),
    Err(err) => println!("Error: {}", err)
  }
}

fn select_port<T: MidiIO>(midi_io: &T, descr: &str) -> Result<T::Port, Box<dyn Error>> {
  let midi_ports = midi_io.ports();

  let result =
    midi_ports.iter().position(|p| {
      midi_io.port_name(p).map_or(false, |s| s == descr)
    }).and_then(|i| {
      midi_ports.get(i)
    }).ok_or(format!("{} cannot be opened", descr))?;

  Ok(result.clone())
}

#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), Box<dyn Error>> {
    println!("test_forward cannot run on Web MIDI");
    Ok(())
}