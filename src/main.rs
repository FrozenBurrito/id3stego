use std::env;
use std::io::Read;
use std::path::Path;
use clap::{command, Arg, ArgAction};
use id3::{Tag, Version, Error, ErrorKind, TagLike};
use id3::frame::EncapsulatedObject;
use infer;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;

// See https://stackoverflow.com/questions/63302814/is-there-a-way-to-disable-enable-the-println-macro
// See also https://veykril.github.io/tlborm/decl-macros/patterns/tt-muncher.html re tt munching
macro_rules! println {
    ($($rest:tt)*) => {
        if !std::env::var("QUIET").is_ok() {
            std::println!($($rest)*);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argument_matches = command!()
        .arg(
            Arg::new("mode")
                .short('m')
                .long("mode")
                .help("\'put\' (or \'insert\') OR \'get\' (or \'extract\')")
                .required(false)
                .action(ArgAction::Set)
        )
        .arg(
            Arg::new("audio_file")
                .short('a')
                .long("audiofile")
                .help("Input audio file of type mp3, wav, or aiff (will not be modified)")
                .required(false)
                .action(ArgAction::Set)
            )
        .arg(
            Arg::new("other_file")
                .short('o')
                .long("otherfile")
                .help("File (any type, size < 16mb) to embed in audio file (will not be modified)")
                .required(false)
                .action(ArgAction::Set)
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Quiet (suppress) all output except errors.")
                .required(false)
                .action(ArgAction::SetTrue)
        ).get_matches();

    let help_msg = "For usage information, type .\\id3stego -h".to_string();

    // if quiet cl arg flag used (-q), set env variable (process) used by println! macro
    // existence checked in println! macro (not value)
    if argument_matches.get_flag("quiet") {
        std::env::set_var("QUIET", "ON");
    }
    
    let mode = argument_matches.get_one::<String>("mode");
    match mode {
        None => {
            eprintln!("Error: No mode type (-m put or -m get) specified.");
            eprintln!("{}", &help_msg);
        }
        Some(mode) => {
            let audio_filename = argument_matches.get_one::<String>("audio_file");
            let other_filename = argument_matches.get_one::<String>("other_file");
            if mode == "put" || mode == "insert" {
                match audio_filename {
                    None => {
                        eprintln!("Error (Put Mode): No audio file (-a filename) specified.");
                        eprintln!("{}", &help_msg);
                    }
                    Some(audio_filename) => {
                        if Path::exists(Path::new(audio_filename)) {
                            println!("Checkpoint (Put Mode): Audio file exists in working directory.");
                            match other_filename {
                                None => {
                                    eprintln!("Error (Put Mode): No other file (-o filename) specified.");
                                    eprintln!("{}", &help_msg);
                                }                                
                                Some(other_filename) => {
                                    if Path::exists(Path::new(other_filename)) {
                                        println!("Checkpoint (Put Mode): Other file exists in working directory.");
                                        match put(audio_filename.to_string(), other_filename.to_string()) {
                                            Ok(output_filename) => {
                                                println!("Checkpoint (Put Mode): Success! {} is {} + {}. All done!", 
                                                    output_filename, &audio_filename, &other_filename);
                                            }
                                            Err(_) => {
                                                eprintln!("{}", &help_msg);
                                            }
                                        }
                                    }
                                    else {
                                        eprintln!("Error (Put Mode): Other file (-o filename) not found in working directory.");
                                        eprintln!("{}", &help_msg);
                                    }
                                }
                            }
                        }
                        else {
                            eprintln!("Error (Put Mode): Audio file (-a filename) not found in working directory.");
                            eprintln!("{}", &help_msg);
                        }
                    }
                }
            }
            else if mode == "get" || mode == "extract" {
                match audio_filename {
                    None => {
                        eprintln!("Error (Get Mode): No audio filename specified.");
                        eprintln!("{}", &help_msg);
                    }
                    Some(audio_filename) => {
                        if Path::exists(Path::new(audio_filename)) {
                            println!("Checkpoint (Get Mode): Audio file exists in working directory.");                     
                            match get(audio_filename.to_string()) {
                                Ok(extracted_filenames_ok) => {
                                    match extracted_filenames_ok {
                                        Some(extracted_filenames) => {
                                            println!("Checkpoint (Get Mode): id3stego extracted the following {} file(s) from {}:", 
                                                &extracted_filenames.len().to_string(), &audio_filename);
                                            for filename in extracted_filenames {
                                                println!("\t- {} saved to working directory as extracted-{}", filename, filename);
                                            }
                                        }
                                        None => {
                                            println!("Checkpoint (Get Mode): No id3stego embedded file(s) found in {}.", 
                                                &audio_filename);                     
                                        }
                                    }
                                    println!("Checkpoint (Get Mode): Success! Note that {} was not modified.", &audio_filename); 
                                }
                                Err(_) => {
                                    println!("{}", &help_msg);
                                }
                            }
                        }
                        else {
                            eprintln!("Error (Get Mode): Audio file (-a filename) not found in working directory.");
                            eprintln!("{}", &help_msg);
                        }
                    }
                }
            }
            else {
                eprintln!("Error: Invalid mode type (-m mode) specified.");
                eprintln!("{}", &help_msg);
            }
        }
    }
    
    // if previously set, remove env (process) variable QUIET
    if std::env::var("QUIET").is_ok() {
        std::env::remove_var("QUIET");
    }
    
    Ok(())

}

fn put(audio_filename: String, other_filename: String) -> Result<String, Box<dyn std::error::Error>> {
    // success: return output_filename as string
    // failure: prints error message, returns err

    let mut output_filename = "output-".to_string();
    output_filename.push_str(&audio_filename);

    // check file-type of audio_filename
    match is_supported_filetype(&audio_filename) {
        Ok(supported_ok) => {
            match supported_ok {
                Some(supported_filetype) => {
                    println!("Checkpoint (Put Mode): Mime-type of {} is \'{}\'.", 
                        &audio_filename, supported_filetype);
                }
                None => {
                    eprintln!("Error (Put Mode): Mime-type of {} must be mp3, wav, or aiff.", 
                        &audio_filename);
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, 
                        "Unsupported audio file (-a audio_file) type.")));
                }
            }
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to determine mime-type of {} (mp3, wav, or aiff required).", 
                &audio_filename);
            return Err(err)
        }
    }

    // open other_filename
    let mut other_file = match std::fs::File::open(&other_filename) {
        Ok(other_file) => {
            println!("Checkpoint (Put Mode): Opening {}.", &other_filename);
            other_file
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to open {}.", &other_filename);
            return Err(Box::new(err))
        }
    };

    // read bytes from other_filename into buffer
    // confirm other_file size < 16mb (maximum id3v2 frame size)
    let mut other_file_buffer = Vec::new();
    let max_frame_size = 16 * 1000000; // 10^6 used instead of 2^20; 1,000,000 vs 1,048,576.
    match other_file.read_to_end(&mut other_file_buffer) {
        Ok(bytes_read) => {
            if bytes_read <= max_frame_size {
                println!("Checkpoint (Put Mode): Reading {} bytes from {} into buffer.", 
                    bytes_read.to_string(), &other_filename);
            }
            else {
                eprintln!("Error (Put Mode): Other file {} exceeds 16mb (id3v2 max frame size).",
                    &other_filename);
                return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, 
                    "Max id3v2 frame size (16mb) exceeded.")));
            }
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to read bytes from {} into buffer.", &other_filename);
            return Err(Box::new(err))
        }
    };

    // if possible, infer mimetype of other_file from buffer
    let other_file_mimetype = match infer::get(&other_file_buffer) {
        Some(kind) => {
            println!("Checkpoint (Put Mode): Inferring mime-type of \'{}\' from buffer as \'{}\'.", 
                &other_filename, kind.mime_type());
            kind.mime_type().to_owned()
        }
        None => {
            println!("Checkpoint (Put Mode): Unable to infer mime-type of {} from buffer.", 
                &other_filename);
            println!("Checkpoint (Put Mode): Using mime-type of \'application/octet-stream\' for {}.",
                &other_filename);
            "application/octet-stream".to_string()
        }
    };

    // copy audio_filename to output_filename
    match std::fs::copy(&audio_filename, &output_filename) {
        Ok(bytes_copied) => {
            println!("Checkpoint (Put Mode): Copying {} to {} ({} bytes).", 
                    &audio_filename, &output_filename, bytes_copied.to_string());
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to copy {}.", &audio_filename);
            return Err(Box::new(err))
        }
    }
    
    // search for id3 tag in output_filename, create if none found
    let mut tag = match Tag::read_from_path(&output_filename) {
        Ok(tag) => {
            println!("Checkpoint (Put Mode): Extracting existing id3v2 tag from {}.", &output_filename);
            tag
        }
        Err(Error{kind: ErrorKind::NoTag, ..}) => {
            println!("Checkpoint (Put Mode): No id3v2 tag in {}.", &output_filename);
            println!("Checkpoint (Put Mode): Creating new id3v2 tag for {}.", &output_filename);            
            Tag::new()
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to find or create id3v2 tag in {}.", &output_filename);
            error_cleanup(&output_filename);
            return Err(Box::new(err))
        }
    };  

    // prepare new frame data
    let frame_mime_type = other_file_mimetype;
    let frame_filename = other_filename.to_owned();
    let frame_data: Vec<u8> = other_file_buffer;
    let mut frame_description_key = "id3stego".to_string();

    // set frame description key to 'id3stego' + random 10 character string 
    // prevent collisions if tag already contains another file previously embedded by id3stego (multi file embedding)
    let rand_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    frame_description_key.push_str(&rand_string);

    // embed buffered data read from other_file into new frame in id3 tag
    println!("Checkpoint (Put Mode): Injecting buffer (data from {}) into new id3v2 frame.", &other_filename);
    println!("Checkpoint (Put Mode): Using frame description key \'{}\'.", &frame_description_key);
    //let frame = match tag.add_frame(
    match tag.add_frame(
        EncapsulatedObject {
            mime_type: frame_mime_type,
            filename: frame_filename,
            description: frame_description_key,
            data: frame_data,
        }) {
            Some(_) => {
                println!("Checkpoint (Put Mode): Existing id3v2 frame found with same frame description key (collision)!");
                println!("Checkpoint (Put Mode): Overwriting existing id3v2 frame with same frame description key.");                
                //frame
            }
            None => {
                println!("Checkpoint (Put Mode): Adding new frame to id3v2 tag.");
            }
        };

    // write tag back to output_file
    match tag.write_to_path(&output_filename, Version::Id3v24) {
        Ok(_) => {
            println!("Checkpoint (Put Mode): Writing id3v2 tag with new frame to {}.", &output_filename);
        }
        Err(err) => {
            eprintln!("Error (Put Mode): Unable to write finalized id3v2 tag to {}.", &output_filename);
            error_cleanup(&output_filename);
            return Err(Box::new(err))
        }
    }
    Ok(output_filename)
} 
 
fn get(audio_filename: String) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    // success: return vector of extracted filenames or none
    // failure: prints error message, returns err    
    let mut extracted_filenames: Vec<String> = Vec::new();

    // check file-type of audio_filename
    match is_supported_filetype(&audio_filename) {
        Ok(supported_ok) => {
            match supported_ok {
                Some(supported_filetype) => {
                        println!("Checkpoint (Get Mode): Mime-type of {} is \'{}\'.", 
                            &audio_filename, supported_filetype); 
                }
                None => {
                    eprintln!("Error (Get Mode): Mime-type of {} must be mp3, wav, or aiff.", 
                        &audio_filename);
                    return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, 
                        "Unsupported audio file (-a audio_file) type.")));
                }
            }
        }
        Err(err) => {
            eprintln!("Error (Get Mode): Unable to determine mime-type of {} (mp3, wav, or aiff required).", 
                &audio_filename);
            return Err(err)
        }
    }
    
    // search for id3 tag in output_filename, ret if none found
    let tag = match Tag::read_from_path(&audio_filename) {
        Ok(tag) => {
            println!("Checkpoint (Get Mode): Extracting existing id3v2 tag from {}.", &audio_filename);
            tag
        }
        Err(err) => match err.kind { 
            ErrorKind::NoTag => {
                println!("Checkpoint (Get Mode): No id3v2 tag in {}. No data found to extract.", &audio_filename);
                return Err(Box::new(err))
            }
            _ => {
                eprintln!("Error (Get Mode): Unable to find id3v2 tag in {}. No data found to extract.", &audio_filename);
                return Err(Box::new(err))
            }
        }
    };  

    // iterate all encapsulated object frames contained in discovered id3v2 tag
    let mut id3stego_frame_count = 0;
    println!("Checkpoint (Get Mode): Searching id3v2 tag for frames containing files previously embedded by id3stego.");
    let mut encapsulated_objects = tag.encapsulated_objects();
    loop {
        match encapsulated_objects.next() {
            Some(frame) => {
                // extract to working directory if frame placed by id3stego (description is 'id3stego')
                if frame.description.contains("id3stego") {                    
                    let mut extracted_filename_with_prefix: String = "extracted-".to_string();
                    extracted_filename_with_prefix.push_str(&frame.filename);

                    extracted_filenames.push(frame.filename.to_owned());
                    //extracted_filenames.push(extracted_filename_with_prefix.to_owned());

                    println!("Checkpoint (Get Mode): Found embedded file {} (\'{}\' of size {} bytes).",
                        &frame.filename, &frame.mime_type, &frame.data.len().to_string());  

                    match std::fs::write(&extracted_filename_with_prefix, &frame.data) {
                        Ok(_) => {
                            println!("Checkpoint (Get Mode): Extracting {} to working directory with name {}.",
                                &frame.filename, extracted_filename_with_prefix);
                        }
                        Err(_) => {
                            eprintln!("Error (Get Mode): Unable to extract {} from {}",
                                &frame.filename, &audio_filename);
                            // do not propagate error, continue iter to next embedded file
                        }
                    };
                    id3stego_frame_count += 1;
                }
            }
            None => {
                println!("Checkpoint (Get Mode): Finished searching id3v2 tag data.");
                break;
            }
        };
    }
    if id3stego_frame_count == 0 {
        Ok(None)
    }
    else {
        Ok(Some(extracted_filenames))
    }
}


fn is_supported_filetype(filename: &String) -> Result<Option<String>, Box<dyn std::error::Error>> {
    // returns mime-type if filename is of type mp3, wav, or aiff.
    // otherwise, returns none or error.
    match infer::get_from_path(filename) {
        Ok(kind_ok) => { 
            match kind_ok { 
                Some(kind) => {
                    if kind.mime_type() == "audio/mpeg" || 
                       kind.mime_type() == "audio/x-wav" || 
                       kind.mime_type() == "audio/x-aiff" {
                        return Ok(Some(kind.mime_type().to_string()))
                    }
                    else {
                        // not of type mp3, wav, or aiff
                        return Ok(None)
                    }
                }
                None => {
                    // no mimetype found
                    return Ok(None)
                }
            }
        }
        Err(err) => {
            // error reading mimetype
            return Err(Box::new(err))
        }
    };
}

fn error_cleanup(filename: &String) /* -> Result<(), Box<dyn std::error::Error>> */ {
    // deletes copied output file if error occurs after making copy.
    // to do:  consider adding directly in put method (only called twice in put function)
    match std::fs::remove_file(filename) {
        Ok(_) => {
            eprintln!("Error (Put Mode): Cleaning up, removing {}.", filename);
            //Ok(())
        }
        Err(_) => {
            eprintln!("Error (Put Mode): Unable to delete (clean up) {}.", filename);
            //return Err(Box::new(err))
            //does not propagate errors (errors handled by put function)
        }
    }
}