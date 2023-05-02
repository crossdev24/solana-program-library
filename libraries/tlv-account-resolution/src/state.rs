//! State transition types

use {
    crate::{
        error::AccountResolutionError,
        pod::{PodAccountMeta, PodSlice, PodSliceMut},
    },
    solana_program::{
        account_info::AccountInfo,
        instruction::{AccountMeta, Instruction},
        program_error::ProgramError,
    },
    spl_type_length_value::{
        discriminator::TlvDiscriminator,
        state::{TlvState, TlvStateBorrowed, TlvStateMut},
    },
};

/// Stateless helper for storing additional accounts required for an instruction.
///
/// This struct works with any `TlvDiscriminator`, and stores the extra accounts
/// needed for that specific instruction, using the given `Discriminator` as the
/// type-length-value `Discriminator`, and then storing all of the given
/// `AccountMeta`s as a zero-copy slice.
///
/// Sample usage:
///
/// ```
/// use {
///     solana_program::{
///         account_info::AccountInfo, instruction::{AccountMeta, Instruction},
///         pubkey::Pubkey
///     },
///     spl_type_length_value::discriminator::{Discriminator, TlvDiscriminator},
///     spl_tlv_account_resolution::state::ExtraAccountMetas,
/// };
///
/// struct MyInstruction;
/// impl TlvDiscriminator for MyInstruction {
///     // Give it a unique discriminator, can also be generated using a hash function
///     const TLV_DISCRIMINATOR: Discriminator = Discriminator::new([1; Discriminator::LENGTH]);
/// }
///
/// // actually put it in the additional required account keys and signer / writable
/// let extra_metas = [
///     AccountMeta::new(Pubkey::new_unique(), false),
///     AccountMeta::new(Pubkey::new_unique(), true),
///     AccountMeta::new_readonly(Pubkey::new_unique(), true),
///     AccountMeta::new_readonly(Pubkey::new_unique(), false),
/// ];
///
/// // assume that this buffer is actually account data, already allocated to `account_size`
/// let account_size = ExtraAccountMetas::size_of(extra_metas.len()).unwrap();
/// let mut buffer = vec![0; account_size];
///
/// // Initialize the structure for your instruction
/// ExtraAccountMetas::init_with_account_metas::<MyInstruction>(&mut buffer, &extra_metas).unwrap();
///
/// // Off-chain, you can add the additional accounts directly from the account data
/// let program_id = Pubkey::new_unique();
/// let mut instruction = Instruction::new_with_bytes(program_id, &[], vec![]);
/// ExtraAccountMetas::add_to_instruction::<MyInstruction>(&mut instruction, &buffer).unwrap();
///
/// // On-chain, you can add the additional accounts *and* account infos
/// let mut cpi_instruction = Instruction::new_with_bytes(program_id, &[], vec![]);
/// let mut cpi_account_infos = vec![]; // assume the other required account infos are already included
/// let remaining_account_infos: &[AccountInfo<'_>] = &[]; // these are the account infos provided to the instruction that are *not* part of any other known interface
/// ExtraAccountMetas::add_to_cpi_instruction::<MyInstruction>(
///     &mut cpi_instruction,
///     &mut cpi_account_infos,
///     &buffer,
///     &remaining_account_infos,
/// );
/// ```
pub struct ExtraAccountMetas;
impl ExtraAccountMetas {
    /// Initialize pod slice data for the given instruction and any type
    /// convertible to account metas
    pub fn init<'a, T: TlvDiscriminator, M>(
        data: &mut [u8],
        convertible_account_metas: &'a [M],
    ) -> Result<(), ProgramError>
    where
        PodAccountMeta: From<&'a M>,
    {
        let mut state = TlvStateMut::unpack(data).unwrap();
        let tlv_size = PodSlice::<PodAccountMeta>::size_of(convertible_account_metas.len())?;
        let bytes = state.alloc::<T>(tlv_size)?;
        let mut extra_account_metas = PodSliceMut::init(bytes)?;
        for account_metas in convertible_account_metas {
            extra_account_metas.push(PodAccountMeta::from(account_metas))?;
        }
        Ok(())
    }

    /// Initialize a TLV entry for the given discriminator, populating the data
    /// with the given account infos
    pub fn init_with_account_infos<T: TlvDiscriminator>(
        data: &mut [u8],
        account_infos: &[AccountInfo<'_>],
    ) -> Result<(), ProgramError> {
        Self::init::<T, AccountInfo>(data, account_infos)
    }

    /// Get the underlying `PodSlice<PodAccountMeta>` from an unpacked TLV
    ///
    /// Due to lifetime annoyances, this function can't just take in the bytes,
    /// since then we would be returning a reference to a locally created
    /// `TlvStateBorrowed`. I hope there's a better way to do this!
    pub fn unpack_with_tlv_state<'a, T: TlvDiscriminator>(
        tlv_state: &'a TlvStateBorrowed,
    ) -> Result<PodSlice<'a, PodAccountMeta>, ProgramError> {
        let bytes = tlv_state.get_bytes::<T>()?;
        PodSlice::<PodAccountMeta>::unpack(bytes)
    }

    /// Initialize a TLV entry for the given discriminator, populating the data
    /// with the given account metas
    pub fn init_with_account_metas<T: TlvDiscriminator>(
        data: &mut [u8],
        account_metas: &[AccountMeta],
    ) -> Result<(), ProgramError> {
        Self::init::<T, AccountMeta>(data, account_metas)
    }

    /// Get the byte size required to hold `num_items` items
    pub fn size_of(num_items: usize) -> Result<usize, ProgramError> {
        Ok(TlvStateBorrowed::get_base_len()
            .saturating_add(PodSlice::<PodAccountMeta>::size_of(num_items)?))
    }

    /// Add the additional account metas to an existing instruction
    pub fn add_to_vec<T: TlvDiscriminator>(
        account_metas: &mut Vec<AccountMeta>,
        data: &[u8],
    ) -> Result<(), ProgramError> {
        let state = TlvStateBorrowed::unpack(data)?;
        let bytes = state.get_bytes::<T>()?;
        let extra_account_metas = PodSlice::<PodAccountMeta>::unpack(bytes)?;
        account_metas.extend(extra_account_metas.data().iter().map(|m| m.into()));
        Ok(())
    }

    /// Add the additional account metas to an existing instruction
    pub fn add_to_instruction<T: TlvDiscriminator>(
        instruction: &mut Instruction,
        data: &[u8],
    ) -> Result<(), ProgramError> {
        Self::add_to_vec::<T>(&mut instruction.accounts, data)
    }

    /// Add the additional account metas and account infos for a CPI
    pub fn add_to_cpi_instruction<'a, T: TlvDiscriminator>(
        cpi_instruction: &mut Instruction,
        cpi_account_infos: &mut Vec<AccountInfo<'a>>,
        data: &[u8],
        account_infos: &[AccountInfo<'a>],
    ) -> Result<(), ProgramError> {
        let state = TlvStateBorrowed::unpack(data)?;
        let bytes = state.get_bytes::<T>()?;
        let extra_account_metas = PodSlice::<PodAccountMeta>::unpack(bytes)?;

        for account_meta in extra_account_metas.data().iter().map(AccountMeta::from) {
            let account_info = account_infos
                .iter()
                .find(|&x| *x.key == account_meta.pubkey)
                .ok_or(AccountResolutionError::IncorrectAccount)?
                .clone();
            cpi_account_infos.push(account_info);
            cpi_instruction.accounts.push(account_meta);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        solana_program::{clock::Epoch, instruction::AccountMeta, pubkey::Pubkey},
        spl_type_length_value::discriminator::Discriminator,
    };

    pub struct TestInstruction;
    impl TlvDiscriminator for TestInstruction {
        const TLV_DISCRIMINATOR: Discriminator = Discriminator::new([1; Discriminator::LENGTH]);
    }

    pub struct TestOtherInstruction;
    impl TlvDiscriminator for TestOtherInstruction {
        const TLV_DISCRIMINATOR: Discriminator = Discriminator::new([2; Discriminator::LENGTH]);
    }

    #[test]
    fn init_with_metas() {
        let metas = [
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];
        let account_size = ExtraAccountMetas::size_of(metas.len()).unwrap();
        let mut buffer = vec![0; account_size];

        ExtraAccountMetas::init_with_account_metas::<TestInstruction>(&mut buffer, &metas).unwrap();

        let mut instruction = Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestInstruction>(&mut instruction, &buffer)
            .unwrap();
        assert_eq!(
            instruction
                .accounts
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>(),
            metas.iter().map(PodAccountMeta::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn init_multiple() {
        let metas = [
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];
        let other_metas = [AccountMeta::new(Pubkey::new_unique(), false)];
        let account_size = ExtraAccountMetas::size_of(metas.len()).unwrap()
            + ExtraAccountMetas::size_of(other_metas.len()).unwrap();
        let mut buffer = vec![0; account_size];

        ExtraAccountMetas::init_with_account_metas::<TestInstruction>(&mut buffer, &metas).unwrap();
        ExtraAccountMetas::init_with_account_metas::<TestOtherInstruction>(
            &mut buffer,
            &other_metas,
        )
        .unwrap();

        let mut instruction = Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestInstruction>(&mut instruction, &buffer)
            .unwrap();
        assert_eq!(
            instruction
                .accounts
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>(),
            metas.iter().map(PodAccountMeta::from).collect::<Vec<_>>()
        );
        let mut instruction = Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestOtherInstruction>(&mut instruction, &buffer)
            .unwrap();
        assert_eq!(
            instruction
                .accounts
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>(),
            other_metas
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn init_mixed() {
        // annoying to setup, but need to test this!
        let pubkey1 = Pubkey::new_unique();
        let mut lamports1 = 0;
        let mut data1 = [];
        let pubkey2 = Pubkey::new_unique();
        let mut lamports2 = 0;
        let mut data2 = [];
        let pubkey3 = Pubkey::new_unique();
        let mut lamports3 = 0;
        let mut data3 = [];
        let owner = Pubkey::new_unique();
        let account_infos = [
            AccountInfo::new(
                &pubkey1,
                false,
                true,
                &mut lamports1,
                &mut data1,
                &owner,
                false,
                Epoch::default(),
            ),
            AccountInfo::new(
                &pubkey2,
                true,
                false,
                &mut lamports2,
                &mut data2,
                &owner,
                false,
                Epoch::default(),
            ),
            AccountInfo::new(
                &pubkey3,
                false,
                false,
                &mut lamports3,
                &mut data3,
                &owner,
                false,
                Epoch::default(),
            ),
        ];
        let metas = [
            AccountMeta::new(Pubkey::new_unique(), false),
            AccountMeta::new(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), true),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
            AccountMeta::new_readonly(Pubkey::new_unique(), false),
        ];
        let account_size = ExtraAccountMetas::size_of(account_infos.len()).unwrap()
            + ExtraAccountMetas::size_of(metas.len()).unwrap();
        let mut buffer = vec![0; account_size];

        ExtraAccountMetas::init_with_account_infos::<TestInstruction>(&mut buffer, &account_infos)
            .unwrap();
        ExtraAccountMetas::init_with_account_metas::<TestOtherInstruction>(&mut buffer, &metas)
            .unwrap();

        let mut instruction = Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestInstruction>(&mut instruction, &buffer)
            .unwrap();
        assert_eq!(
            instruction
                .accounts
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>(),
            account_infos
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>()
        );

        let mut instruction = Instruction::new_with_bytes(Pubkey::new_unique(), &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestOtherInstruction>(&mut instruction, &buffer)
            .unwrap();
        assert_eq!(
            instruction
                .accounts
                .iter()
                .map(PodAccountMeta::from)
                .collect::<Vec<_>>(),
            metas.iter().map(PodAccountMeta::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cpi_instruction() {
        // annoying to setup, but need to test this!
        let pubkey1 = Pubkey::new_unique();
        let mut lamports1 = 0;
        let mut data1 = [];
        let pubkey2 = Pubkey::new_unique();
        let mut lamports2 = 0;
        let mut data2 = [];
        let pubkey3 = Pubkey::new_unique();
        let mut lamports3 = 0;
        let mut data3 = [];
        let owner = Pubkey::new_unique();
        let account_infos = [
            AccountInfo::new(
                &pubkey1,
                false,
                true,
                &mut lamports1,
                &mut data1,
                &owner,
                false,
                Epoch::default(),
            ),
            AccountInfo::new(
                &pubkey2,
                true,
                false,
                &mut lamports2,
                &mut data2,
                &owner,
                false,
                Epoch::default(),
            ),
            AccountInfo::new(
                &pubkey3,
                false,
                false,
                &mut lamports3,
                &mut data3,
                &owner,
                false,
                Epoch::default(),
            ),
        ];
        let account_size = ExtraAccountMetas::size_of(account_infos.len()).unwrap();
        let mut buffer = vec![0; account_size];

        ExtraAccountMetas::init_with_account_infos::<TestInstruction>(&mut buffer, &account_infos)
            .unwrap();

        // make an instruction to check later
        let program_id = Pubkey::new_unique();
        let mut instruction = Instruction::new_with_bytes(program_id, &[], vec![]);
        ExtraAccountMetas::add_to_instruction::<TestInstruction>(&mut instruction, &buffer)
            .unwrap();

        // mess around with the account infos to make it harder
        let mut messed_account_infos = account_infos.to_vec();
        let pubkey4 = Pubkey::new_unique();
        let mut lamports4 = 0;
        let mut data4 = [];
        messed_account_infos.push(AccountInfo::new(
            &pubkey4,
            false,
            true,
            &mut lamports4,
            &mut data4,
            &owner,
            false,
            Epoch::default(),
        ));
        let pubkey5 = Pubkey::new_unique();
        let mut lamports5 = 0;
        let mut data5 = [];
        messed_account_infos.push(AccountInfo::new(
            &pubkey5,
            false,
            true,
            &mut lamports5,
            &mut data5,
            &owner,
            false,
            Epoch::default(),
        ));
        messed_account_infos.swap(0, 4);
        messed_account_infos.swap(1, 2);

        let mut cpi_instruction = Instruction::new_with_bytes(program_id, &[], vec![]);
        let mut cpi_account_infos = vec![];
        ExtraAccountMetas::add_to_cpi_instruction::<TestInstruction>(
            &mut cpi_instruction,
            &mut cpi_account_infos,
            &buffer,
            &messed_account_infos,
        )
        .unwrap();

        assert_eq!(cpi_instruction, instruction);
        assert_eq!(cpi_account_infos.len(), account_infos.len());
        for (a, b) in std::iter::zip(cpi_account_infos, account_infos) {
            assert_eq!(a.key, b.key);
            assert_eq!(a.is_signer, b.is_signer);
            assert_eq!(a.is_writable, b.is_writable);
        }
    }
}
