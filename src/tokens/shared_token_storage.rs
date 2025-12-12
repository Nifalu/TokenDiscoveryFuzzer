// src/shared_token_storage.rs

use std::ffi::CString;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering};

use libafl_bolts::shmem::unix_shmem::{MmapShMem, MmapShMemProvider};
use libafl_bolts::shmem::{ShMem, ShMemId, ShMemProvider};

const HEADER_SIZE: usize = 8;

pub struct SharedTokenStorage {
    shmem: MmapShMem,
    name: CString,
    is_owner: bool,
    last_sequence: u32,
}

impl SharedTokenStorage {
    pub fn get_or_create(name: &str, max_tokens: usize, max_token_len: usize) -> Result<Self, String> {
        let size = HEADER_SIZE + max_tokens * (2 + max_token_len);
        let mut provider = MmapShMemProvider::new().map_err(|e| e.to_string())?;
        let c_name = CString::new(format!("/{}", name)).map_err(|e| e.to_string())?;

        let (shmem, is_owner) = match provider.new_shmem_with_id(size, name) {
            Ok(mut shmem) => {
                shmem.fill(0);
                (shmem, true)
            }
            Err(_) => {
                let id = ShMemId::from_string(&format!("/{}", name));
                let shmem = provider.shmem_from_id_and_size(id, size).map_err(|e| e.to_string())?;
                (shmem, false)
            }
        };

        Ok(Self { shmem, name: c_name, is_owner, last_sequence: 0 })
    }

    pub fn is_owner(&self) -> bool { self.is_owner }

    fn sequence(&self) -> &AtomicU32 {
        unsafe { &*(self.shmem.as_ptr() as *const AtomicU32) }
    }

    fn count_ptr(&self) -> *mut u32 {
        unsafe { self.shmem.as_ptr().add(4) as *mut u32 }
    }

    fn data_ptr(&self) -> *mut u8 {
        unsafe { self.shmem.as_ptr().add(HEADER_SIZE) as *mut u8 }
    }

    pub fn write_tokens(&mut self, tokens: &[Vec<u8>]) {
        self.sequence().fetch_add(1, Ordering::Release);

        let mut offset = 0;
        let mut written_count = 0u32;  // Track actual count
        let data = self.data_ptr();
        let max_data = self.shmem.len() - HEADER_SIZE;

        for token in tokens {
            let len = token.len();
            if offset + 2 + len > max_data { break; }
            unsafe {
                ptr::copy_nonoverlapping((len as u16).to_le_bytes().as_ptr(), data.add(offset), 2);
                offset += 2;
                ptr::copy_nonoverlapping(token.as_ptr(), data.add(offset), len);
                offset += len;
            }
            written_count += 1;  // Only count what we actually wrote
        }

        unsafe { ptr::write_volatile(self.count_ptr(), written_count); }
        self.sequence().fetch_add(1, Ordering::Release);
    }

    pub fn read_tokens(&mut self) -> Option<Vec<Vec<u8>>> {
        let seq1 = self.sequence().load(Ordering::Acquire);
        if seq1 % 2 == 1 || seq1 == self.last_sequence { return None; }

        let count = unsafe { ptr::read_volatile(self.count_ptr()) } as usize;
        let data = self.data_ptr();
        let max_data = self.shmem.len() - HEADER_SIZE;

        let mut tokens = Vec::with_capacity(count);
        let mut offset = 0;

        for _ in 0..count {
            if offset + 2 > max_data { break; }
            unsafe {
                let mut len_bytes = [0u8; 2];
                ptr::copy_nonoverlapping(data.add(offset), len_bytes.as_mut_ptr(), 2);
                let len = u16::from_le_bytes(len_bytes) as usize;
                offset += 2;
                if offset + len > max_data { break; }
                tokens.push(std::slice::from_raw_parts(data.add(offset), len).to_vec());
                offset += len;
            }
        }

        let seq2 = self.sequence().load(Ordering::Acquire);
        if seq1 != seq2 { return None; }

        self.last_sequence = seq1;
        Some(tokens)
    }
}

impl Drop for SharedTokenStorage {
    fn drop(&mut self) {
        if self.is_owner {
            unsafe { libc::shm_unlink(self.name.as_ptr()); }
        }
    }
}