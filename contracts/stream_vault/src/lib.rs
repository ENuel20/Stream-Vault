#![no_std]
// Contract entry points take `env` plus explicit params by Soroban convention.
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token::TokenClient, Address, Env, Symbol,
    Vec,
};

// ---------------------------------------------------------------------------
// Storage types
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Counter,
    Stream(u64),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    pub sender: Address,
    pub recipient: Address,
    pub token: Address,
    pub deposit: i128,
    pub withdrawn: i128,
    pub start: u64,
    pub cliff: u64,
    pub end: u64,
    pub cancellable: bool,
    pub cancelled: bool,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelResult {
    pub recipient_payout: i128,
    pub sender_refund: i128,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    InvalidTime = 5,
    StreamNotFound = 6,
    StreamCancelled = 7,
    NothingToWithdraw = 8,
    NotCancellable = 9,
}

// ---------------------------------------------------------------------------
// TTL helpers
// ---------------------------------------------------------------------------

const INSTANCE_TTL: u32 = 30 * 24 * 60 * 60; // ~30 days in ledgers (approx)
const PERSISTENT_TTL: u32 = 30 * 24 * 60 * 60;

fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL, INSTANCE_TTL);
}

fn extend_stream(env: &Env, stream_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Stream(stream_id),
        PERSISTENT_TTL,
        PERSISTENT_TTL,
    );
}

fn require_initialized(env: &Env) -> Result<Address, VaultError> {
    env.storage()
        .instance()
        .get::<_, Address>(&DataKey::Admin)
        .ok_or(VaultError::NotInitialized)
}

fn load_stream(env: &Env, stream_id: u64) -> Result<Stream, VaultError> {
    env.storage()
        .persistent()
        .get::<_, Stream>(&DataKey::Stream(stream_id))
        .ok_or(VaultError::StreamNotFound)
}

fn save_stream(env: &Env, stream_id: u64, stream: &Stream) {
    env.storage()
        .persistent()
        .set(&DataKey::Stream(stream_id), stream);
    extend_stream(env, stream_id);
}

/// Linear vesting with cliff gate. Pure function — also used by tests.
pub fn vested_amount_for(stream: &Stream, now: u64) -> i128 {
    if now < stream.cliff {
        return 0;
    }
    if now >= stream.end {
        return stream.deposit;
    }
    if stream.end <= stream.start {
        return 0;
    }
    let elapsed: i128 = (now - stream.start) as i128;
    let duration: i128 = (stream.end - stream.start) as i128;
    if duration <= 0 {
        return 0;
    }
    // deposit * elapsed / duration (checked, saturating safe)
    stream
        .deposit
        .checked_mul(elapsed)
        .and_then(|v| v.checked_div(duration))
        .unwrap_or(0)
        .clamp(0, stream.deposit)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct StreamVault;

#[contractimpl]
impl StreamVault {
    /// One-time initializer. Sets the admin address.
    pub fn initialize(env: Env, admin: Address) -> Result<(), VaultError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(VaultError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Counter, &0u64);
        extend_instance(&env);
        env.events().publish((Symbol::new(&env, "init"),), admin);
        Ok(())
    }

    /// Returns the admin address.
    pub fn admin(env: Env) -> Result<Address, VaultError> {
        extend_instance(&env);
        require_initialized(&env)
    }

    /// Returns the total number of streams created.
    pub fn stream_count(env: Env) -> Result<u64, VaultError> {
        require_initialized(&env)?;
        extend_instance(&env);
        Ok(env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::Counter)
            .unwrap_or(0))
    }

    /// Create a new payment stream. Pulls `amount` of `token` from `sender`
    /// into this contract and returns the new stream id.
    pub fn create_stream(
        env: Env,
        sender: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        start: u64,
        cliff: u64,
        end: u64,
        cancellable: bool,
    ) -> Result<u64, VaultError> {
        require_initialized(&env)?;
        sender.require_auth();

        if amount <= 0 {
            return Err(VaultError::InvalidAmount);
        }
        if sender == recipient {
            return Err(VaultError::Unauthorized);
        }
        // Time validation: start < end, start <= cliff <= end, start not in past.
        let now = env.ledger().timestamp();
        if end <= start {
            return Err(VaultError::InvalidTime);
        }
        if cliff < start || cliff > end {
            return Err(VaultError::InvalidTime);
        }
        if start < now {
            return Err(VaultError::InvalidTime);
        }

        // Pull funds into the vault.
        let client = TokenClient::new(&env, &token);
        client.transfer(&sender, &env.current_contract_address(), &amount);

        // Allocate id.
        let count: u64 = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::Counter)
            .unwrap_or(0);
        let stream_id = count;
        env.storage()
            .instance()
            .set(&DataKey::Counter, &(count + 1));

        let stream = Stream {
            sender: sender.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            deposit: amount,
            withdrawn: 0,
            start,
            cliff,
            end,
            cancellable,
            cancelled: false,
            created_at: now,
        };
        save_stream(&env, stream_id, &stream);
        extend_instance(&env);

        env.events().publish(
            (Symbol::new(&env, "created"),),
            (stream_id, sender, recipient, token, amount),
        );
        Ok(stream_id)
    }

    /// Recipient withdraws all currently vested, un-withdrawn funds.
    /// Returns the amount transferred.
    pub fn withdraw(env: Env, stream_id: u64) -> Result<i128, VaultError> {
        require_initialized(&env)?;
        let mut stream = load_stream(&env, stream_id)?;
        if stream.cancelled {
            return Err(VaultError::StreamCancelled);
        }
        stream.recipient.require_auth();

        let now = env.ledger().timestamp();
        let vested = vested_amount_for(&stream, now);
        let owed = vested - stream.withdrawn;
        if owed <= 0 {
            return Err(VaultError::NothingToWithdraw);
        }

        // Effects before interaction.
        stream.withdrawn += owed;
        save_stream(&env, stream_id, &stream);
        extend_instance(&env);

        let client = TokenClient::new(&env, &stream.token);
        client.transfer(&env.current_contract_address(), &stream.recipient, &owed);

        env.events().publish(
            (Symbol::new(&env, "withdraw"),),
            (stream_id, stream.recipient.clone(), owed),
        );
        Ok(owed)
    }

    /// Cancel a stream. Only the sender may cancel (recipient must use
    /// withdraw). Vested funds go to the recipient, remainder to sender.
    pub fn cancel(env: Env, stream_id: u64) -> Result<CancelResult, VaultError> {
        require_initialized(&env)?;
        let mut stream = load_stream(&env, stream_id)?;
        if stream.cancelled {
            return Err(VaultError::StreamCancelled);
        }
        if !stream.cancellable {
            return Err(VaultError::NotCancellable);
        }
        stream.sender.require_auth();

        let now = env.ledger().timestamp();
        let vested = vested_amount_for(&stream, now);
        let recipient_payout = vested - stream.withdrawn;
        let sender_refund = stream.deposit - vested;

        stream.cancelled = true;
        stream.withdrawn = vested;
        save_stream(&env, stream_id, &stream);
        extend_instance(&env);

        let client = TokenClient::new(&env, &stream.token);
        let contract_addr = env.current_contract_address();
        if recipient_payout > 0 {
            client.transfer(&contract_addr, &stream.recipient, &recipient_payout);
        }
        if sender_refund > 0 {
            client.transfer(&contract_addr, &stream.sender, &sender_refund);
        }

        env.events().publish(
            (Symbol::new(&env, "cancel"),),
            (
                stream_id,
                stream.sender.clone(),
                recipient_payout,
                sender_refund,
            ),
        );
        Ok(CancelResult {
            recipient_payout,
            sender_refund,
        })
    }

    // -- Views ----------------------------------------------------------

    pub fn get_stream(env: Env, stream_id: u64) -> Result<Stream, VaultError> {
        require_initialized(&env)?;
        extend_stream(&env, stream_id);
        load_stream(&env, stream_id)
    }

    pub fn vested(env: Env, stream_id: u64) -> Result<i128, VaultError> {
        require_initialized(&env)?;
        let stream = load_stream(&env, stream_id)?;
        extend_stream(&env, stream_id);
        Ok(vested_amount_for(&stream, env.ledger().timestamp()))
    }

    pub fn withdrawable(env: Env, stream_id: u64) -> Result<i128, VaultError> {
        require_initialized(&env)?;
        let stream = load_stream(&env, stream_id)?;
        extend_stream(&env, stream_id);
        if stream.cancelled {
            return Ok(0);
        }
        Ok(vested_amount_for(&stream, env.ledger().timestamp()) - stream.withdrawn)
    }

    /// Paginated stream ids: returns up to `limit` ids starting at `offset`.
    pub fn list_streams(env: Env, offset: u64, limit: u64) -> Result<Vec<u64>, VaultError> {
        require_initialized(&env)?;
        extend_instance(&env);
        let count: u64 = env
            .storage()
            .instance()
            .get::<_, u64>(&DataKey::Counter)
            .unwrap_or(0);
        let limit = limit.min(50);
        let mut out = Vec::new(&env);
        let mut i = offset;
        while i < count && (i - offset) < limit {
            out.push_back(i);
            i += 1;
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{token::StellarAssetClient, Address, Env};

    fn setup() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);
        let vault_id = env.register(StreamVault, ());
        (env, admin, sender, recipient, vault_id)
    }

    #[test]
    fn test_vesting_math() {
        let s = Stream {
            sender: Address::generate(&Env::default()),
            recipient: Address::generate(&Env::default()),
            token: Address::generate(&Env::default()),
            deposit: 1_000,
            withdrawn: 0,
            start: 100,
            cliff: 100,
            end: 200,
            cancellable: true,
            cancelled: false,
            created_at: 90,
        };
        assert_eq!(vested_amount_for(&s, 50), 0);
        assert_eq!(vested_amount_for(&s, 100), 0);
        assert_eq!(vested_amount_for(&s, 150), 500);
        assert_eq!(vested_amount_for(&s, 200), 1_000);
        assert_eq!(vested_amount_for(&s, 999), 1_000);

        // Cliff gating.
        let mut c = s.clone();
        c.cliff = 150;
        assert_eq!(vested_amount_for(&c, 140), 0);
        assert_eq!(vested_amount_for(&c, 150), 500);
    }

    #[test]
    fn test_initialize_twice_fails() {
        let (env, admin, _, _, _) = setup();
        let id = env.register(StreamVault, ());
        let c = StreamVaultClient::new(&env, &id);
        c.initialize(&admin);
        let res = c.try_initialize(&admin);
        assert!(res.is_err());
    }

    #[test]
    fn test_create_stream_validates_time() {
        let (env, admin, sender, recipient, _) = setup();
        let token = Address::generate(&env);
        let id = env.register(StreamVault, ());
        let c = StreamVaultClient::new(&env, &id);
        c.initialize(&admin);
        // end <= start
        let res = c.try_create_stream(
            &sender, &recipient, &token, &100i128, &200u64, &200u64, &100u64, &true,
        );
        assert!(res.is_err());
        // zero amount
        let now = env.ledger().timestamp();
        let res = c.try_create_stream(
            &sender,
            &recipient,
            &token,
            &0i128,
            &(now + 10),
            &(now + 10),
            &(now + 100),
            &true,
        );
        assert!(res.is_err());
    }

    #[test]
    fn test_full_lifecycle_with_stellar_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let sender = Address::generate(&env);
        let recipient = Address::generate(&env);

        // Deploy a Stellar Asset Contract token for testing.
        let sac_admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(sac_admin.clone());
        let token_addr = token_id.address();
        // Mint to sender (auths mocked).
        StellarAssetClient::new(&env, &token_addr).mint(&sender, &10_000);

        let vault_id = env.register(StreamVault, ());
        let vault = StreamVaultClient::new(&env, &vault_id);
        vault.initialize(&admin);

        let now = env.ledger().timestamp();
        let start = now + 10;
        let end = start + 100;
        let sid = vault.create_stream(
            &sender,
            &recipient,
            &token_addr,
            &1_000i128,
            &start,
            &start,
            &end,
            &true,
        );
        assert_eq!(sid, 0);

        // Before start: nothing withdrawable (as recipient auth mocked).
        let mut li = env.ledger().get();
        li.timestamp = start;
        env.ledger().set(li);
        assert_eq!(vault.withdrawable(&sid), 0);

        // Midway: half vested.
        let mut li = env.ledger().get();
        li.timestamp = start + 50;
        env.ledger().set(li);
        assert_eq!(vault.vested(&sid), 500);
        let got = vault.withdraw(&sid);
        assert_eq!(got, 500);

        // Cancel: recipient gets 0 new (already withdrew vested at t=mid),
        // sender refunded unvested.
        let res = vault.cancel(&sid);
        assert_eq!(res.recipient_payout, 0);
        assert_eq!(res.sender_refund, 500);
        assert_eq!(vault.withdrawable(&sid), 0);
    }
}
