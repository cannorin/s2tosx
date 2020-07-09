use std::io::{stdin, stdout, Write, ErrorKind};
use std::error::Error;
use std::result::Result;
use midir::{MidiInput, MidiOutput, MidiIO, Ignore};

fn exn (msg: String) -> Box<dyn Error> {
  Box::new(std::io::Error::new(ErrorKind::Other, msg))
}

trait ResultExn<T, E: 'static + Error> {
  fn upcast_err(self) -> Result<T, Box<dyn Error>>;
}

impl<T, E: 'static + Error> ResultExn<T, E> for Result<T, E> {
  fn upcast_err(self) -> Result<T, Box<dyn Error>> {
    match self {
      Ok (x) => Ok (x),
      Err (e) => Err (Box::new(e))
    }
  }
}

#[cfg(not(target_arch="wasm32"))]
fn run() -> Result<(), Box<dyn Error>> {
  let midi_in1 = MidiInput::new("midir forwarding input 1").upcast_err();
  let midi_in2 = MidiInput::new("midir forwarding input 2").upcast_err();
  let midi_out1 = MidiOutput::new("midir forwarding output 1").upcast_err();
  let midi_out2 = MidiOutput::new("midir forwarding output 2").upcast_err();

  match (midi_in1, midi_in2, midi_out1, midi_out2) {
    (Err(e), _,_,_) | (_, Err(e),_,_) | (_,_,Err(e),_) | (_,_,_,Err(e)) => { Err(e) }
    (Ok (mut midi_in1), Ok (mut midi_in2), Ok (midi_out1), Ok (midi_out2)) => {
      midi_in1.ignore(Ignore::None);
      midi_in2.ignore(Ignore::None);
      let tk_in_port = select_port(&midi_in1, "Traktor Kontrol S3 MIDI");
      let ddj_out_port = select_port(&midi_out1, "Pioneer DDJ-SX");
      let ddj_in_port = select_port(&midi_in2, "Pioneer DDJ-SX");
      let tk_out_port = select_port(&midi_out2, "Traktor Kontrol S3 MIDI");

      match (tk_in_port, tk_out_port, ddj_in_port, ddj_out_port) {
        (Err(e), _,_,_) | (_, Err(e),_,_) | (_,_,Err(e),_) | (_,_,_,Err(e)) => { Err(e) }
        (Ok (tk_in_port), Ok (tk_out_port), Ok (ddj_in_port), Ok (ddj_out_port)) => {
          println!("\nOpening connections");
          let in_port_name = midi_in1.port_name(&tk_in_port)?;
          let out_port_name = midi_out1.port_name(&ddj_out_port)?;

          match (midi_out1.connect(&ddj_out_port, "midir-forward1").upcast_err(),
                 midi_out2.connect(&tk_out_port,  "midir-forward2").upcast_err()) {
            (Err(e), _) | (_, Err(e)) => { Err(e) },
            (Ok(mut ddj_conn_out), Ok(mut tk_conn_out)) => {
              let ddj_to_tk = midi_in2.connect(&ddj_in_port, "midir-forward2", move |stamp, data, _| {
                let status = data[0];
                let msg = status / 16u8;
                let ch  = status % 16u8;
                if msg == 0x8 {
                  let mut new_data : Vec<u8> = Vec::from(data);
                  new_data[0] = 0x90 + ch;
                  let new_data = new_data.as_slice();
                  tk_conn_out.send(new_data).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                }
                else if msg == 0x9 || status == 0xBB {}
                else {
                  println!("{}: <= {:?}", stamp, data);
                }
              }, ()).upcast_err();

              let tk_to_ddj = midi_in1.connect(&tk_in_port, "midir-forward1", move |stamp, data, _| {
                let status = data[0];
                let msg = status / 16u8;
                let ch  = status % 16u8;

                print!("{}: {:?} ", stamp, data);
                // jog spin
                if msg == 0xB && data.len() == 3 && data[1] == 30u8 {
                  let new_ch = (if ch % 2 == 0 {ch / 2u8} else {ch}) / 2u8;
                  let new_value = {
                    let orig = data[2];
                    let zoom = 4;
                    if orig < 64u8 { 63u8 - (63u8 - orig) * zoom }
                    else { (orig - 64u8) * zoom + 64u8 }
                  };
                  let new_data : [u8;3] = [
                    0xB0u8 + new_ch,
                    if ch % 2 == 0 { 0x1Fu8 } else { 0x22u8 },
                    new_value
                  ];
                  println!("=> {:?} (jog spin)", new_data);
                  ddj_conn_out.send(&new_data).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                }
                // jog touch
                else if msg == 0x9 && data.len() == 3 && data[1] == 20u8 {
                  let new_ch = (if ch % 2 == 0 {ch / 2u8} else {ch}) / 2u8;
                  let new_data : [u8;3] = [
                    0x90u8 + new_ch,
                    if ch % 2 == 0 { 0x67u8 } else { 0x36u8 },
                    data[2]
                  ];
                  println!("=> {:?} (jog touch)", new_data);
                  ddj_conn_out.send(&new_data).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                }
                
                // other
                else {
                  println!("=> {:?} (path through)", data);
                  ddj_conn_out.send(&data).unwrap_or_else(|_| println!("Error when forwarding message ..."));
                }
              }, ()).upcast_err();

              match (ddj_to_tk, tk_to_ddj) {
                (Err(e), _) | (_, Err(e)) => Err(e),
                (Ok(_ddj_to_tk), Ok(_tk_to_ddj)) => {
                  println!("Connections open, forwarding from '{}' to '{}' (press enter to exit) ...", in_port_name, out_port_name);

                  let mut input = String::new();
                  stdin().read_line(&mut input)?; // wait for next enter key press

                  println!("Closing connections");

                  Ok(())
                }
              }
            }
          }
        }
      }
    }
  }
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
    }).ok_or_else(|| format!("{} cannot be opened", descr));

  match result {
    Ok (port) => { Ok (port.clone()) },
    Err (msg) => { Err (exn (msg)) }
  }
}

#[cfg(target_arch = "wasm32")]
fn run() -> Result<(), Box<dyn Error>> {
  Err(exn("cannot run on Web MIDI"))
}