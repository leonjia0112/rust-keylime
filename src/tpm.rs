extern crate base64;
extern crate flate2;

use super::*;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use openssl::sha;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::io::Read;
use std::process::Command;
use std::process::Output;
use std::str;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use tempfile::NamedTempFile;

const MAX_TRY: usize = 10;
const RETRY_SLEEP: Duration = Duration::from_millis(50);
const TPM_IO_ERROR: i32 = 5;
const RETRY: usize = 4;
const EXIT_SUCCESS: i32 = 0;

static EMPTYMASK: &'static str = "1";

/*
 * tpm data struct for tpmdata.json file IO
 */
#[derive(Serialize, Deserialize, Debug)]
struct TpmData {
    aik_pw: String,
    ek: String,
    owner_pw: String,
    aik_handle: String,
    aikmod: String,
    aikpriv: String,
    aik: String,
}

/***************************************************************
ftpm_initialize.py
Following are function from tpm_initialize.py program      
*****************************************************************/
/*
input: content key in tpmdata
return: deserlized json object
getting the tpm data struct and convert it to a json value object to access to a particular field inside the tpm data file
*/
fn get_tpm_metadata(content_key: String) -> Option<String> {
    let t_data = read_tpm_data().unwrap();
    let t_data_json_value = serde_json::to_value(t_data).unwrap();
    Some(t_data_json_value[content_key].to_string())
}

/*
 * input: none
 * return: tpmdata in json object
 *
 * read in tpmdata.json file and convert it to a pre-defined struct
 */
fn read_tpm_data() -> Result<TpmData, Box<Error>> {
    let file = File::open("tpmdata.json")?;
    let data: TpmData = serde_json::from_reader(file)?;
    Ok(data)
}

/*
 * input: None
 * output: boolean
 *
 * If tpm is a tpm elumator, return true, other wise return false
 */
pub fn is_vtpm() -> Option<bool> {
    match common::STUB_VTPM {
        true => Some(true),
        false => {
            let tpm_manufacturer = get_tpm_manufacturer();

            // ******* //
            println!("tpm manufacturer: {:?}", tpm_manufacturer);
            Some(tpm_manufacturer.unwrap() == "ETHZ")
        }
    }
}

/*
 * getting the tpm manufacturer information
 * is_vtpm helper method
 */
fn get_tpm_manufacturer() -> Option<String> {
    // let return_output = run("getcapability -cap 1a".to_string());

    let placeholder = String::from("ETHZ");
    Some(placeholder)
}

/***************************************************************
tpm_quote.py
Following are function from tpm_quote.py program    
*****************************************************************/

pub fn create_quote(
    nonce: String,
    data: String,
    mut pcrmask: String,
) -> Option<String> {
    let quote_path = NamedTempFile::new().unwrap();
    let key_handle = get_tpm_metadata("aik_handle".to_string());
    let aik_password = get_tpm_metadata("aik_pw".to_string());

    if pcrmask == "".to_string() {
        pcrmask = EMPTYMASK.to_string();
    }

    if !(data == "".to_string()) {
        let pcrmask_int: i32 = pcrmask.parse().unwrap();
        pcrmask =
            format!("0x{}", (pcrmask_int + (1 << common::TPM_DATA_PCR)));
        let mut command = format!("pcrreset -ix {}", common::TPM_DATA_PCR);

        // RUN
        run(command, EXIT_SUCCESS, true, false, String::new());

        // sha1 hash data
        let mut hasher = sha::Sha1::new();
        hasher.update(data.as_bytes());
        let data_sha1_hash = hasher.finish();

        command = format!(
            "extend -ix {} -ic {}",
            common::TPM_DATA_PCR,
            hex::encode(data_sha1_hash),
        );

        run(command, EXIT_SUCCESS, true, false, String::new());
    }

    // store quote into the temp file that will be extracted later
    let command = format!(
        "tpmquote -hk {} -pwdk {} -bm {} -nonce {} -noverify -oq {}",
        key_handle.unwrap(),
        aik_password.unwrap(),
        pcrmask,
        nonce,
        quote_path.path().to_str().unwrap().to_string(),
    );

    let (_return_output, _exit_code, quote_raw) = run(
        command,
        EXIT_SUCCESS,
        true,
        false,
        quote_path.path().to_string_lossy().to_string(),
    );

    let mut quote_return = String::from("r");
    quote_return.push_str(&base64_zlib_encode(quote_raw.unwrap()));
    Some(quote_return)
}

pub fn create_deep_quote(
    nonce: String,
    data: String,
    mut pcrmask: String,
    mut vpcrmask: String,
) -> Option<String> {
    let quote_path = NamedTempFile::new().unwrap();
    let key_handle = get_tpm_metadata("aik_handle".to_string());
    let aik_password = get_tpm_metadata("aik_pw".to_string());
    let owner_password = get_tpm_metadata("owner_pw".to_string());

    if pcrmask == "".to_string() {
        pcrmask = EMPTYMASK.to_string();
    }

    if vpcrmask == "".to_string() {
        vpcrmask = EMPTYMASK.to_string();
    }

    if !(data == "".to_string()) {
        let vpcrmask_int: i32 = vpcrmask.parse().unwrap();
        vpcrmask =
            format!("0x{}", (vpcrmask_int + (1 << common::TPM_DATA_PCR)));
        let mut command = format!("pcrreset -ix {}", common::TPM_DATA_PCR);

        // RUN
        run(command, EXIT_SUCCESS, true, false, String::new());

        let mut hasher = sha::Sha1::new();
        hasher.update(data.as_bytes());
        let data_sha1_hash = hasher.finish();

        command = format!(
            "extend -ix {} -ic {}",
            common::TPM_DATA_PCR,
            hex::encode(data_sha1_hash),
        );

        // RUN
        run(command, EXIT_SUCCESS, true, false, String::new());
    }

    // store quote into the temp file that will be extracted later
    let command = format!(
        "deepquote -vk {} -hm {} -vm {} -nonce {} -pwdo {} -pwdk {} -oq {}",
        key_handle.unwrap(),
        pcrmask,
        vpcrmask,
        nonce,
        owner_password.unwrap(),
        aik_password.unwrap(),
        quote_path.path().to_str().unwrap(),
    );

    // RUN
    let (_return_output, _exit_code, quote_raw) = run(
        command,
        EXIT_SUCCESS,
        true,
        false,
        quote_path.path().to_string_lossy().to_string(),
    );

    let mut quote_return = String::from("d");
    quote_return.push_str(&base64_zlib_encode(quote_raw.unwrap()));
    Some(quote_return)
}

/*
 * Input: string to be encoded
 * Output: encoded string output
 *
 * Use zlib to compression the input and encoded with base64 encoding
 * method
 */
fn base64_zlib_encode(data: String) -> String {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());

    match e.write_all(data.as_bytes()) {
        Ok(_) => {
            let compressed_bytes = e.finish();
            match compressed_bytes {
                Ok(e) => base64::encode(&e),
                Err(_) => String::from(""),
            }
        }
        Err(_) => String::from("Encode Fail!"),
    }
}

pub fn check_mask(ima_mask: String, ima_pcr: usize) -> bool {
    if ima_mask.is_empty() {
        return false;
    }
    let ima_mask_int: i32 = ima_mask.parse().unwrap();
    match (1 << ima_pcr) & ima_mask_int {
        0 => return false,
        _ => return true,
    }
}

/***************************************************************
tpm_nvram.py
Following are function from tpm_nvram.py program    
*****************************************************************/

/***************************************************************
tpm_exec.py
Following are function from tpm_exec.py program
*****************************************************************/

/*
 * Input:
 *     cmd: command to be executed
 *     except_code: return code that needs extra handling
 *     raise_on_error: raise exception/panic while encounter error option
 *     lock: lock engage option
 *     output_path: file output location
 * return:
 *     tuple contains (standard output, return code, and file output)
 *
 * execute tpm command through shell commands and return the execution
 * result in a tuple
 */
fn run<'a>(
    cmd: String,
    except_code: i32,
    raise_on_error: bool,
    _lock: bool,
    output_path: String,
) -> (Vec<u8>, Option<i32>, Option<String>) {
    /* stubbing  placeholder */

    // tokenize input command
    let words: Vec<&str> = cmd.split(" ").collect();
    let mut number_tries = 0;
    let args = &words[1..words.len()];

    // setup environment variable
    let mut env_vars: HashMap<String, String> = HashMap::new();
    for (key, value) in env::vars() {
        // println!("{}: {}", key, value);
        env_vars.insert(key.to_string(), value.to_string());
    }

    env_vars.insert("TPM_SERVER_PORT".to_string(), "9998".to_string());
    env_vars.insert("TPM_SERVER_NAME".to_string(), "localhost".to_string());
    env_vars
        .get_mut("PATH")
        .unwrap()
        .push_str(common::TPM_TOOLS_PATH);

    let mut t_diff: u64 = 0;
    let mut output: Output;

    loop {
        let t0 = SystemTime::now();

        // command execution
        output = Command::new(&cmd)
            .args(args)
            .envs(&env_vars)
            .output()
            .expect("failed to execute process");

        // measure execution time
        match t0.duration_since(t0) {
            Ok(t_delta) => t_diff = t_delta.as_secs(),
            Err(_) => {}
        }
        info!("Time cost: {}", t_diff);

        // assume the system is linux
        println!("number tries: {:?}", number_tries);

        match output.status.code().unwrap() {
            TPM_IO_ERROR => {
                number_tries += 1;
                if number_tries >= MAX_TRY {
                    error!("TPM appears to be in use by another application.  Keylime is incompatible with other TPM TSS applications like trousers/tpm-tools. Please uninstall or disable.");
                    break;
                }

                info!(
                    "Failed to call TPM {}/{} times, trying again in {} seconds...",
                    number_tries,
                    MAX_TRY,
                    RETRY,
                );

                thread::sleep(RETRY_SLEEP);
            }
            _ => break,
        }
    }

    let return_output = output.stdout;
    let return_code = output.status.code();

    if return_code.unwrap() == except_code && raise_on_error {
        panic!(
            "Command: {} returned {}, expected {}, output {}",
            cmd,
            return_code.unwrap(),
            except_code.to_string(),
            String::from_utf8_lossy(&return_output),
        );
    }

    let mut file_output: String = String::new();

    match read_file_output_path(output_path) {
        Ok(content) => file_output = content,
        Err(_) => {}
    }

    /* metric output placeholder */

    (return_output, return_code, Some(file_output))
}

/*
 * input: file name
 * return: the content of the file int Result<>
 *
 * run method helper method
 * read in the file and  return the content of the file into a Result enum
 */
fn read_file_output_path(output_path: String) -> std::io::Result<String> {
    let mut file = File::open(output_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_vtpm() {
        let return_value = is_vtpm();
        assert_eq!(return_value.unwrap(), true);
    }
}