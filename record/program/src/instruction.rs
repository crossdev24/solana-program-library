//! Program instructions

use {
    crate::id,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program_error::ProgramError,
        pubkey::Pubkey,
    },
    std::mem::size_of,
};

/// Instructions supported by the program
#[derive(Clone, Debug, PartialEq)]
pub enum RecordInstruction<'a> {
    /// Create a new record
    ///
    /// Accounts expected by this instruction:
    ///
    /// 0. `[writable]` Record account, must be uninitialized
    /// 1. `[]` Record authority
    Initialize,

    /// Write to the provided record account
    ///
    /// Accounts expected by this instruction:
    ///
    /// 0. `[writable]` Record account, must be previously initialized
    /// 1. `[signer]` Current record authority
    Write {
        /// Offset to start writing record, expressed as `u64`.
        offset: u64,
        /// Data to replace the existing record data
        data: &'a [u8],
    },

    /// Update the authority of the provided record account
    ///
    /// Accounts expected by this instruction:
    ///
    /// 0. `[writable]` Record account, must be previously initialized
    /// 1. `[signer]` Current record authority
    /// 2. `[]` New record authority
    SetAuthority,

    /// Close the provided record account, draining lamports to recipient
    /// account
    ///
    /// Accounts expected by this instruction:
    ///
    /// 0. `[writable]` Record account, must be previously initialized
    /// 1. `[signer]` Record authority
    /// 2. `[]` Receiver of account lamports
    CloseAccount,
}

impl<'a> RecordInstruction<'a> {
    /// Unpacks a byte buffer into a [RecordInstruction].
    pub fn unpack(input: &'a [u8]) -> Result<Self, ProgramError> {
        let (&tag, rest) = input
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;
        Ok(match tag {
            0 => Self::Initialize,
            1 => {
                const U32_BYTES: usize = 4;
                const U64_BYTES: usize = 8;
                let offset = rest
                    .get(..U64_BYTES)
                    .and_then(|slice| slice.try_into().ok())
                    .map(u64::from_le_bytes)
                    .ok_or(ProgramError::InvalidInstructionData)?;
                let (length, data) = rest[U64_BYTES..].split_at(U32_BYTES);
                let length = u32::from_le_bytes(
                    length
                        .try_into()
                        .map_err(|_| ProgramError::InvalidInstructionData)?,
                ) as usize;

                Self::Write {
                    offset,
                    data: &data[..length],
                }
            }
            2 => Self::SetAuthority,
            3 => Self::CloseAccount,
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }

    /// Packs a [RecordInstruction] into a byte buffer.
    pub fn pack(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size_of::<Self>());
        match self {
            Self::Initialize => buf.push(0),
            Self::Write { offset, data } => {
                buf.push(1);
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
            }
            Self::SetAuthority => buf.push(2),
            Self::CloseAccount => buf.push(3),
        };
        buf
    }
}

/// Create a `RecordInstruction::Initialize` instruction
pub fn initialize(record_account: &Pubkey, authority: &Pubkey) -> Instruction {
    Instruction {
        program_id: id(),
        accounts: vec![
            AccountMeta::new(*record_account, false),
            AccountMeta::new_readonly(*authority, false),
        ],
        data: RecordInstruction::Initialize.pack(),
    }
}

/// Create a `RecordInstruction::Write` instruction
pub fn write(record_account: &Pubkey, signer: &Pubkey, offset: u64, data: &[u8]) -> Instruction {
    Instruction {
        program_id: id(),
        accounts: vec![
            AccountMeta::new(*record_account, false),
            AccountMeta::new_readonly(*signer, true),
        ],
        data: RecordInstruction::Write { offset, data }.pack(),
    }
}

/// Create a `RecordInstruction::SetAuthority` instruction
pub fn set_authority(
    record_account: &Pubkey,
    signer: &Pubkey,
    new_authority: &Pubkey,
) -> Instruction {
    Instruction {
        program_id: id(),
        accounts: vec![
            AccountMeta::new(*record_account, false),
            AccountMeta::new_readonly(*signer, true),
            AccountMeta::new_readonly(*new_authority, false),
        ],
        data: RecordInstruction::SetAuthority.pack(),
    }
}

/// Create a `RecordInstruction::CloseAccount` instruction
pub fn close_account(record_account: &Pubkey, signer: &Pubkey, receiver: &Pubkey) -> Instruction {
    Instruction {
        program_id: id(),
        accounts: vec![
            AccountMeta::new(*record_account, false),
            AccountMeta::new_readonly(*signer, true),
            AccountMeta::new(*receiver, false),
        ],
        data: RecordInstruction::CloseAccount.pack(),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*, crate::state::tests::TEST_DATA, solana_program::program_error::ProgramError,
        spl_pod::bytemuck::pod_bytes_of,
    };

    #[test]
    fn serialize_initialize() {
        let instruction = RecordInstruction::Initialize;
        let expected = vec![0];
        assert_eq!(instruction.pack(), expected);
        assert_eq!(RecordInstruction::unpack(&expected).unwrap(), instruction);
    }

    #[test]
    fn serialize_write() {
        let data = pod_bytes_of(&TEST_DATA);
        let offset = 0u64;
        let instruction = RecordInstruction::Write { offset: 0, data };
        let mut expected = vec![1];
        expected.extend_from_slice(&offset.to_le_bytes());
        expected.extend_from_slice(&(data.len() as u32).to_le_bytes());
        expected.extend_from_slice(data);
        assert_eq!(instruction.pack(), expected);
        assert_eq!(RecordInstruction::unpack(&expected).unwrap(), instruction);
    }

    #[test]
    fn serialize_set_authority() {
        let instruction = RecordInstruction::SetAuthority;
        let expected = vec![2];
        assert_eq!(instruction.pack(), expected);
        assert_eq!(RecordInstruction::unpack(&expected).unwrap(), instruction);
    }

    #[test]
    fn serialize_close_account() {
        let instruction = RecordInstruction::CloseAccount;
        let expected = vec![3];
        assert_eq!(instruction.pack(), expected);
        assert_eq!(RecordInstruction::unpack(&expected).unwrap(), instruction);
    }

    #[test]
    fn deserialize_invalid_instruction() {
        let mut expected = vec![12];
        expected.append(&mut pod_bytes_of(&TEST_DATA).to_vec());
        let err: ProgramError = RecordInstruction::unpack(&expected).unwrap_err();
        assert_eq!(err, ProgramError::InvalidInstructionData);
    }
}
