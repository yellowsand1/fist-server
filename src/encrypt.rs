use anyhow::Result;
use crypto::{aes, blockmodes, buffer, symmetriccipher};
use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use crypto::symmetriccipher::{BlockDecryptor, BlockEncryptor, Decryptor, Encryptor, SynchronousStreamCipher};

use crate::SETTINGS;

const IV: [u8; 16] = [29, 153, 21, 5, 126, 118, 184, 57, 99, 160, 45, 169, 178, 57, 246, 186];

pub async fn encrypt(data: &str) -> Result<String, symmetriccipher::SymmetricCipherError> {
    let data = data.as_bytes();
    let key = SETTINGS.read().await.get_string("key").unwrap();
    let key = pad_key(&key).await.unwrap();
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);
    let mut encryptor = aes::cbc_encryptor(
        aes::KeySize::KeySize128,
        &key,
        &IV,
        blockmodes::PkcsPadding,
    );
    loop {
        let result = encryptor.encrypt(&mut read_buffer, &mut write_buffer, true)?;
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }
    Ok(base64::encode(final_result))
}

pub async fn decrypt(encrypted_data: &str) -> Result<String, symmetriccipher::SymmetricCipherError> {
    crypto(encrypted_data, false).await
}

async fn crypto(data: &str, encrypt: bool) -> Result<String, symmetriccipher::SymmetricCipherError> {
    // let data = base64::decode(data.to_string()).unwrap();
    let data = data.as_bytes();
    // let key = SETTINGS.read().await.get_string("key").unwrap();
    let key = "1q2w#E$R";
    let key = pad_key(&key).await.unwrap();
    let mut final_result = Vec::<u8>::new();
    let mut read_buffer = buffer::RefReadBuffer::new(data);
    let mut buffer = [0; 4096];
    let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);
    loop {
        let result;
        if encrypt {
            let mut decryptor = aes::cbc_decryptor(
                aes::KeySize::KeySize128,
                &key,
                &IV,
                blockmodes::PkcsPadding);
            result = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true)?;
        } else {
            let mut encryptor = aes::cbc_encryptor(
                aes::KeySize::KeySize128,
                &key,
                &IV,
                blockmodes::PkcsPadding);
            result = encryptor.encrypt(&mut read_buffer, &mut write_buffer, true)?;
        }
        final_result.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));
        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }
    let res = base64::encode(&final_result);
    Ok(res)
}

const KEY_LENGTH: usize = 16;

async fn pad_key(key: &str) -> Result<Vec<u8>> {
    let key_bytes = key.as_bytes();
    let mut padded_key = Vec::with_capacity(KEY_LENGTH);
    let repeated_key = key_bytes.repeat((KEY_LENGTH + key_bytes.len() - 1) / key_bytes.len());
    padded_key.extend_from_slice(&repeated_key[..KEY_LENGTH]);
    Ok(padded_key)
}