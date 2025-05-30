// Copyright (C) 2025 Ryan Daum <ryan.daum@gmail.com> This program is free
// software: you can redistribute it and/or modify it under the terms of the GNU
// General Public License as published by the Free Software Foundation, version
// 3.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with
// this program. If not, see <https://www.gnu.org/licenses/>.
//

use std::io::Write;
use std::str::FromStr;

use age::{
    Decryptor, Encryptor, Recipient as AgeRecipient,
    secrecy::ExposeSecret,
    x25519::{Identity, Recipient},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ssh_key::public::PublicKey;
use std::io::Read;
use tracing::{error, warn};

use crate::vm::builtins::BfRet::Ret;
use crate::vm::builtins::{BfCallState, BfErr, BfRet, BuiltinFunction, world_state_bf_err};
use moor_compiler::offset_for_builtin;
use moor_var::{E_ARGS, Sequence};
use moor_var::{E_INVARG, E_TYPE, Variant};
use moor_var::{v_list, v_string};

/// Function: list age_generate_keypair()
///
/// Generates a new X25519 keypair for use with age encryption.
/// Returns a list containing two strings: the public key and the private key.
/// Both are encoded as Bech32 strings (age1... for public keys, AGE-SECRET-KEY-1... for private keys).
///
/// Example:
/// ```moo
/// keypair = age_generate_keypair();
/// public_key = keypair[1];
/// private_key = keypair[2];
/// ```
fn bf_age_generate_keypair(bf_args: &mut BfCallState<'_>) -> Result<BfRet, BfErr> {
    if !bf_args.args.is_empty() {
        return Err(BfErr::ErrValue(
            E_ARGS.msg("age_generate_keypair() does not take any arguments"),
        ));
    }

    // Check for programmer permissions
    bf_args
        .task_perms()
        .map_err(world_state_bf_err)?
        .check_programmer()
        .map_err(world_state_bf_err)?;

    // Generate a new X25519 identity (keypair)
    let identity = Identity::generate();
    let public_key = identity.to_public().to_string();
    let private_key = identity.to_string().expose_secret().to_string();

    Ok(Ret(v_list(&[v_string(public_key), v_string(private_key)])))
}

/// Function: str age_encrypt(str message, list recipients)
///
/// Encrypts a message using age encryption for one or more recipients.
///
/// Arguments:
/// - message: The string to encrypt
/// - recipients: A list of recipient public keys (either age X25519 keys or SSH public keys)
///
/// Returns:
/// - A base64-encoded encrypted message
///
/// Example:
/// ```moo
/// encrypted = age_encrypt("secret message", {"age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p"});
/// ```
fn bf_age_encrypt(bf_args: &mut BfCallState<'_>) -> Result<BfRet, BfErr> {
    if bf_args.args.len() != 2 {
        return Err(BfErr::ErrValue(
            E_ARGS.msg("age_encrypt() requires exactly two arguments"),
        ));
    }

    // Check for programmer permissions
    bf_args
        .task_perms()
        .map_err(world_state_bf_err)?
        .check_programmer()
        .map_err(world_state_bf_err)?;

    // Get the message to encrypt
    let message = match bf_args.args[0].variant() {
        Variant::Str(s) => s.as_str(),
        _ => {
            return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                format!(
                    "age_encrypt() first argument must be a string, was {}",
                    bf_args.args[0].type_code().to_literal()
                )
            })));
        }
    };

    // Get the recipients list
    let recipients_list = match bf_args.args[1].variant() {
        Variant::List(l) => l,
        _ => {
            return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                format!(
                    "age_encrypt() second argument must be a list, was {}",
                    bf_args.args[1].type_code().to_literal()
                )
            })));
        }
    };

    if recipients_list.is_empty() {
        return Err(BfErr::ErrValue(
            E_INVARG.msg("age_encrypt() requires at least one recipient"),
        ));
    }

    // Process each recipient
    let mut recipients = Vec::new();
    for recipient_var in recipients_list.iter() {
        let recipient_str = match recipient_var.variant() {
            Variant::Str(s) => s.as_str(),
            _ => {
                return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                    format!(
                        "age_encrypt() recipient must be a string, was {}",
                        recipient_var.type_code().to_literal()
                    )
                })));
            }
        };

        if let Ok(x25519_recipient) = recipient_str.parse::<Recipient>() {
            recipients.push(Box::new(x25519_recipient) as Box<dyn AgeRecipient>);
            continue;
        }
        let ssh_key = PublicKey::from_openssh(recipient_str).map_err(|_| {
            warn!("Failed to parse SSH key");
            BfErr::ErrValue(E_INVARG.msg("age_encrypt() failed to parse SSH key"))
        })?;

        let ssh_recipient = age::ssh::Recipient::from_str(&ssh_key.to_string()).map_err(|_| {
            warn!("Failed to create age recipient from SSH key");
            BfErr::ErrValue(
                E_INVARG.msg("age_encrypt() failed to create age recipient from SSH key"),
            )
        })?;
        recipients.push(Box::new(ssh_recipient) as Box<dyn AgeRecipient>);
    }

    // Create an encryptor with the recipients
    let encryptor =
        Encryptor::with_recipients(recipients.iter().map(|r| r.as_ref())).map_err(|_| {
            error!("Failed to create encryptor");
            BfErr::ErrValue(E_INVARG.msg("age_encrypt() failed to create encryptor"))
        })?;

    // Encrypt the message
    let mut encrypted = Vec::new();
    let mut writer = encryptor.wrap_output(&mut encrypted).map_err(|e| {
        error!("Failed to create encryption writer: {}", e);
        BfErr::ErrValue(E_INVARG.msg("age_encrypt() failed to create encryption writer"))
    })?;

    writer
        .write_all(message.as_bytes())
        .and_then(|_| writer.finish())
        .map_err(|e| {
            error!("Failed to write message for encryption: {}", e);
            BfErr::ErrValue(E_INVARG.msg("age_encrypt() failed to write message for encryption"))
        })?;

    // Base64 encode the encrypted data
    let encoded = BASE64.encode(&encrypted);

    Ok(Ret(v_string(encoded)))
}

/// Function: str age_decrypt(str encrypted_message, list private_keys)
///
/// Decrypts an age-encrypted message using one or more private keys.
///
/// Arguments:
/// - encrypted_message: A base64-encoded encrypted message
/// - private_keys: A list of private keys to try for decryption
///
/// Returns:
/// - The decrypted message as a string
///
/// Example:
/// ```moo
/// decrypted = age_decrypt(encrypted, {"AGE-SECRET-KEY-1QUWM2RNFSA5NQVVMRKD7MMVWWGVPZ2F4XPKQ3RZWDWGWXQUKXPFQSZJ9DA"});
/// ```
fn bf_age_decrypt(bf_args: &mut BfCallState<'_>) -> Result<BfRet, BfErr> {
    if bf_args.args.len() != 2 {
        return Err(BfErr::ErrValue(
            E_ARGS.msg("age_decrypt() requires exactly two arguments"),
        ));
    }

    // Check for programmer permissions
    bf_args
        .task_perms()
        .map_err(world_state_bf_err)?
        .check_programmer()
        .map_err(world_state_bf_err)?;

    // Get the encrypted message
    let encrypted_b64 = match bf_args.args[0].variant() {
        Variant::Str(s) => s.as_str(),
        _ => {
            return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                format!(
                    "age_decrypt() first argument must be a string, was {}",
                    bf_args.args[0].type_code().to_literal()
                )
            })));
        }
    };

    // Decode the base64 message
    let encrypted = match BASE64.decode(encrypted_b64) {
        Ok(data) => data,
        Err(_) => {
            warn!("Invalid base64 data for decryption");
            return Err(BfErr::ErrValue(
                E_INVARG.msg("age_decrypt() failed to decode base64 data"),
            ));
        }
    };

    // Get the private keys list
    let private_keys_list = match bf_args.args[1].variant() {
        Variant::List(l) => l,
        _ => {
            return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                format!(
                    "age_decrypt() second argument must be a list, was {}",
                    bf_args.args[1].type_code().to_literal()
                )
            })));
        }
    };

    if private_keys_list.is_empty() {
        return Err(BfErr::ErrValue(
            E_INVARG.msg("age_decrypt() requires at least one private key"),
        ));
    }

    // Process each private key
    let mut identities = Vec::new();
    for key_var in private_keys_list.iter() {
        let key_str = match key_var.variant() {
            Variant::Str(s) => s.as_str(),
            _ => {
                return Err(BfErr::ErrValue(E_TYPE.with_msg(|| {
                    format!(
                        "age_decrypt() private key must be a string, was {}",
                        key_var.type_code().to_literal()
                    )
                })));
            }
        };

        // Try to parse as an age identity
        match key_str.parse::<Identity>() {
            Ok(identity) => {
                identities.push(Box::new(identity) as Box<dyn age::Identity>);
            }
            Err(_) => {
                // Try to parse as an SSH private key
                match ssh_key::PrivateKey::from_openssh(key_str) {
                    Ok(ssh_key) => {
                        // Convert to OpenSSH format string
                        let ssh_key_str = ssh_key
                            .to_openssh(ssh_key::LineEnding::LF)
                            .unwrap_or_default();
                        // Use from_buffer to create the identity
                        match age::ssh::Identity::from_buffer(
                            std::io::Cursor::new(ssh_key_str),
                            None,
                        ) {
                            Ok(ssh_identity) => {
                                identities.push(Box::new(ssh_identity) as Box<dyn age::Identity>);
                            }
                            Err(_) => {
                                warn!("Failed to create age identity from SSH key");
                                // Continue to try other keys
                            }
                        }
                    }
                    Err(_) => {
                        warn!("Invalid private key format");
                        // Continue to try other keys
                    }
                }
            }
        }
    }

    if identities.is_empty() {
        warn!("No valid identities found for decryption");
        return Err(BfErr::ErrValue(
            E_INVARG.msg("age_decrypt() no valid identities found for decryption"),
        ));
    }
    let decryptor = Decryptor::new_buffered(&encrypted[..]).map_err(|_| {
        error!("Failed to create decryptor");
        BfErr::ErrValue(E_INVARG.msg("age_decrypt() failed to create decryptor"))
    })?;

    // Create a decryptor
    let mut reader = decryptor
        .decrypt(identities.iter().map(|i| i.as_ref()))
        .map_err(|_| {
            warn!("Failed to create decryptor");
            BfErr::ErrValue(E_INVARG.msg("age_decrypt() failed to create decryptor"))
        })?;
    let mut decrypted = String::new();
    match reader.read_to_string(&mut decrypted) {
        Ok(_) => Ok(Ret(v_string(decrypted))),
        Err(_) => {
            warn!("Decrypted data is not valid UTF-8");
            Err(BfErr::ErrValue(
                E_INVARG.msg("age_decrypt() decrypted data is not valid UTF-8"),
            ))
        }
    }
}

pub(crate) fn register_bf_age_crypto(builtins: &mut [Box<BuiltinFunction>]) {
    builtins[offset_for_builtin("age_generate_keypair")] = Box::new(bf_age_generate_keypair);
    builtins[offset_for_builtin("age_encrypt")] = Box::new(bf_age_encrypt);
    builtins[offset_for_builtin("age_decrypt")] = Box::new(bf_age_decrypt);
}
