use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext};

const DEFAULT_MESSAGE_CHAR_LIMIT: u16 = 280;
const MIN_MESSAGE_CHAR_LIMIT: u16 = 32;
const MAX_MESSAGE_CHAR_LIMIT: u16 = 1_024;
const DEFAULT_BATCH_WINDOW_SECONDS: u32 = 30;
const MAX_BATCH_WINDOW_SECONDS: u32 = 3_600;
const MAX_HANDLE_LEN: usize = 32;
const MAX_SLUG_LEN: usize = 64;
const MAX_TITLE_LEN: usize = 80;
const DOMAIN_CLAIM_WINDOW_SECS: i64 = 86_400;
const MAX_NAMED_DOMAIN_CLAIMS_PER_WINDOW: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
pub enum AccountStatus {
    Active,
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
pub enum DomainKind {
    Public,
    Private,
    Dm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
pub enum DomainRole {
    Owner,
    Member,
}

#[derive(Clone, Debug, PartialEq, Eq, spacetimedb::SpacetimeType)]
pub struct DmLookup {
    domain_id: u64,
    title: String,
    participant_account_ids: Vec<u64>,
}

#[spacetimedb::table(
    accessor = account,
    public,
    index(accessor = accounts_by_handle, btree(columns = [handle]))
)]
pub struct Account {
    #[primary_key]
    #[auto_inc]
    account_id: u64,
    handle: String,
    created_at: Timestamp,
    status: AccountStatus,
}

#[spacetimedb::table(
    accessor = account_key,
    public,
    index(accessor = account_keys_by_account_id, btree(columns = [account_id]))
)]
pub struct AccountKey {
    #[primary_key]
    key_identity: Identity,
    account_id: u64,
    added_by_identity: Identity,
    created_at: Timestamp,
    revoked_at: Option<Timestamp>,
}

#[spacetimedb::table(
    accessor = domain,
    public,
    index(accessor = domains_by_creator_account_id, btree(columns = [created_by_account_id])),
    index(accessor = domains_by_kind, btree(columns = [kind]))
)]
pub struct Domain {
    #[primary_key]
    #[auto_inc]
    domain_id: u64,
    kind: DomainKind,
    slug: String,
    title: String,
    created_by_account_id: u64,
    created_at: Timestamp,
    message_char_limit: u16,
}

#[spacetimedb::table(
    accessor = domain_member,
    public,
    index(accessor = domain_members_by_domain_id, btree(columns = [domain_id])),
    index(accessor = domain_members_by_account_id, btree(columns = [account_id]))
)]
pub struct DomainMember {
    #[primary_key]
    #[auto_inc]
    membership_id: u64,
    domain_id: u64,
    account_id: u64,
    role: DomainRole,
    joined_at: Timestamp,
}

#[spacetimedb::table(
    accessor = message,
    public,
    index(accessor = messages_by_domain_id, btree(columns = [domain_id])),
    index(accessor = messages_by_author_account_id, btree(columns = [author_account_id])),
    index(accessor = messages_by_reply_to_message_id, btree(columns = [reply_to_message_id]))
)]
pub struct Message {
    #[primary_key]
    #[auto_inc]
    message_id: u64,
    domain_id: u64,
    domain_sequence: u64,
    author_account_id: u64,
    authenticated_key_identity: Identity,
    body: String,
    created_at: Timestamp,
    reply_to_message_id: Option<u64>,
}

#[spacetimedb::table(
    accessor = subscription,
    public,
    index(
        accessor = subscriptions_by_subscriber_account_id,
        btree(columns = [subscriber_account_id])
    ),
    index(accessor = subscriptions_by_domain_id, btree(columns = [domain_id]))
)]
pub struct Subscription {
    #[primary_key]
    #[auto_inc]
    subscription_id: u64,
    subscriber_account_id: u64,
    domain_id: u64,
    batch_window_seconds: u32,
    active: bool,
    created_at: Timestamp,
    updated_at: Timestamp,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn create_account(ctx: &ReducerContext, handle: String) -> Result<(), String> {
    ensure_sender_has_no_active_account(ctx)?;
    validate_handle(ctx, &handle)?;

    let account = ctx.db.account().insert(Account {
        account_id: 0,
        handle,
        created_at: ctx.timestamp,
        status: AccountStatus::Active,
    });

    ctx.db.account_key().insert(AccountKey {
        key_identity: ctx.sender(),
        account_id: account.account_id,
        added_by_identity: ctx.sender(),
        created_at: ctx.timestamp,
        revoked_at: None,
    });

    Ok(())
}

#[spacetimedb::view(accessor = my_dm_domains, public)]
pub fn my_dm_domains(ctx: &ViewContext) -> Vec<DmLookup> {
    let Some(sender_account_id) = active_account_id_for_view_identity(ctx, ctx.sender()) else {
        return vec![];
    };

    ctx.db
        .domain_member()
        .domain_members_by_account_id()
        .filter(sender_account_id)
        .filter_map(|membership| {
            let domain = ctx.db.domain().domain_id().find(membership.domain_id)?;
            if domain.kind != DomainKind::Dm {
                return None;
            }

            Some(DmLookup {
                domain_id: domain.domain_id,
                title: domain.title,
                participant_account_ids: dm_member_account_ids_for_view(ctx, domain.domain_id),
            })
        })
        .collect()
}

#[spacetimedb::reducer]
pub fn add_account_key(ctx: &ReducerContext, key_identity: Identity) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;

    if ctx
        .db
        .account_key()
        .key_identity()
        .find(key_identity)
        .is_some()
    {
        return Err("That key identity is already bound to an account.".to_string());
    }

    ctx.db.account_key().insert(AccountKey {
        key_identity,
        account_id: sender_account.account_id,
        added_by_identity: ctx.sender(),
        created_at: ctx.timestamp,
        revoked_at: None,
    });

    Ok(())
}

#[spacetimedb::reducer]
pub fn revoke_account_key(ctx: &ReducerContext, key_identity: Identity) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let mut key = ctx
        .db
        .account_key()
        .key_identity()
        .find(key_identity)
        .ok_or_else(|| "That key identity is not bound to any account.".to_string())?;

    if key.account_id != sender_account.account_id {
        return Err("You can only revoke keys for your own account.".to_string());
    }
    if key.revoked_at.is_some() {
        return Err("That key identity has already been revoked.".to_string());
    }

    key.revoked_at = Some(ctx.timestamp);
    ctx.db.account_key().key_identity().update(key);
    Ok(())
}

#[spacetimedb::reducer]
pub fn create_domain(
    ctx: &ReducerContext,
    kind: DomainKind,
    slug: String,
    title: String,
    message_char_limit: u16,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    if kind == DomainKind::Dm {
        return Err("DMs must be created with create_dm, not create_domain.".to_string());
    }
    validate_domain_fields(kind, &slug, &title, message_char_limit)?;
    ensure_named_domain_claim_allowed(ctx, sender_account.account_id)?;
    ensure_slug_is_available(ctx, &slug)?;

    let domain = ctx.db.domain().insert(Domain {
        domain_id: 0,
        kind,
        slug,
        title,
        created_by_account_id: sender_account.account_id,
        created_at: ctx.timestamp,
        message_char_limit: normalize_message_char_limit(message_char_limit)?,
    });

    insert_domain_member(
        ctx,
        domain.domain_id,
        sender_account.account_id,
        DomainRole::Owner,
    )?;

    Ok(())
}

#[spacetimedb::reducer]
pub fn create_dm(
    ctx: &ReducerContext,
    recipient_account_ids: Vec<u64>,
    title: String,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let participant_account_ids =
        canonicalize_dm_participants(ctx, sender_account.account_id, recipient_account_ids)?;
    validate_title(&title)?;

    if find_existing_dm(ctx, &participant_account_ids).is_some() {
        return Ok(());
    }

    let domain = ctx.db.domain().insert(Domain {
        domain_id: 0,
        kind: DomainKind::Dm,
        slug: String::new(),
        title,
        created_by_account_id: sender_account.account_id,
        created_at: ctx.timestamp,
        message_char_limit: DEFAULT_MESSAGE_CHAR_LIMIT,
    });

    insert_domain_member(
        ctx,
        domain.domain_id,
        sender_account.account_id,
        DomainRole::Owner,
    )?;
    for account_id in participant_account_ids {
        if account_id == sender_account.account_id {
            continue;
        }
        insert_domain_member(ctx, domain.domain_id, account_id, DomainRole::Member)?;
    }

    Ok(())
}

#[spacetimedb::reducer]
pub fn create_dm_with_domain_id(
    ctx: &ReducerContext,
    domain_id: u64,
    recipient_account_ids: Vec<u64>,
    title: String,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let participant_account_ids =
        canonicalize_dm_participants(ctx, sender_account.account_id, recipient_account_ids)?;
    validate_title(&title)?;

    if let Some(domain) = ctx.db.domain().domain_id().find(domain_id) {
        if domain.kind != DomainKind::Dm {
            return Err("That domain_id does not refer to a DM domain.".to_string());
        }
        if dm_member_account_ids(ctx, domain.domain_id) != participant_account_ids {
            return Err("That domain_id refers to a different DM participant set.".to_string());
        }
        return Ok(());
    }

    if let Some(existing_dm) = find_existing_dm(ctx, &participant_account_ids) {
        return Err(format!(
            "A DM for that participant set already exists as domain_id {}.",
            existing_dm.domain_id
        ));
    }

    create_dm(ctx, participant_account_ids, title)
}

#[spacetimedb::reducer]
pub fn join_domain(ctx: &ReducerContext, domain_id: u64) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let domain = require_domain(ctx, domain_id)?;

    if domain.kind != DomainKind::Public {
        return Err("Only public domains can be joined directly.".to_string());
    }
    if has_domain_membership(ctx, domain_id, sender_account.account_id) {
        return Ok(());
    }

    insert_domain_member(
        ctx,
        domain_id,
        sender_account.account_id,
        DomainRole::Member,
    )
}

#[spacetimedb::reducer]
pub fn add_domain_member(
    ctx: &ReducerContext,
    domain_id: u64,
    account_id: u64,
    role: DomainRole,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let domain = require_domain(ctx, domain_id)?;

    require_domain_owner(ctx, domain_id, sender_account.account_id)?;
    require_active_account(ctx, account_id)?;

    if domain.kind == DomainKind::Dm {
        return Err("DM membership is fixed after creation.".to_string());
    }
    if has_domain_membership(ctx, domain_id, account_id) {
        return Err("That account is already a member of the domain.".to_string());
    }

    insert_domain_member(ctx, domain_id, account_id, role)
}

#[spacetimedb::reducer]
pub fn remove_domain_member(
    ctx: &ReducerContext,
    domain_id: u64,
    account_id: u64,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let domain = require_domain(ctx, domain_id)?;

    require_domain_owner(ctx, domain_id, sender_account.account_id)?;

    if domain.kind == DomainKind::Dm {
        return Err("DM membership cannot be changed after creation.".to_string());
    }

    let membership = find_domain_membership(ctx, domain_id, account_id)
        .ok_or_else(|| "That account is not a member of the domain.".to_string())?;

    ctx.db
        .domain_member()
        .membership_id()
        .delete(&membership.membership_id);
    Ok(())
}

#[spacetimedb::reducer]
pub fn post_message(
    ctx: &ReducerContext,
    domain_id: u64,
    body: String,
    reply_to_message_id: Option<u64>,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let domain = require_domain(ctx, domain_id)?;
    validate_message_body(&body, domain.message_char_limit as usize)?;

    if !can_post_in_domain(ctx, &domain, sender_account.account_id) {
        return Err("You are not allowed to post in that domain.".to_string());
    }

    if let Some(reply_to_message_id) = reply_to_message_id {
        let reply_target = ctx
            .db
            .message()
            .message_id()
            .find(reply_to_message_id)
            .ok_or_else(|| "Reply target message does not exist.".to_string())?;

        if reply_target.domain_id != domain_id {
            return Err("Replies must stay within the same domain.".to_string());
        }
    }

    ctx.db.message().insert(Message {
        message_id: 0,
        domain_id,
        domain_sequence: next_domain_sequence(ctx, domain_id),
        author_account_id: sender_account.account_id,
        authenticated_key_identity: ctx.sender(),
        body,
        created_at: ctx.timestamp,
        reply_to_message_id,
    });

    Ok(())
}

#[spacetimedb::reducer]
pub fn subscribe_domain(
    ctx: &ReducerContext,
    domain_id: u64,
    batch_window_seconds: u32,
) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let domain = require_domain(ctx, domain_id)?;
    let batch_window_seconds = normalize_batch_window(batch_window_seconds)?;

    if !can_read_domain(ctx, &domain, sender_account.account_id) {
        return Err("You are not allowed to subscribe to that domain.".to_string());
    }

    if let Some(mut subscription) = find_subscription(ctx, sender_account.account_id, domain_id) {
        subscription.batch_window_seconds = batch_window_seconds;
        subscription.active = true;
        subscription.updated_at = ctx.timestamp;
        ctx.db.subscription().subscription_id().update(subscription);
        return Ok(());
    }

    ctx.db.subscription().insert(Subscription {
        subscription_id: 0,
        subscriber_account_id: sender_account.account_id,
        domain_id,
        batch_window_seconds,
        active: true,
        created_at: ctx.timestamp,
        updated_at: ctx.timestamp,
    });

    Ok(())
}

#[spacetimedb::reducer]
pub fn unsubscribe_domain(ctx: &ReducerContext, domain_id: u64) -> Result<(), String> {
    let sender_account = require_active_sender_account(ctx)?;
    let mut subscription = find_subscription(ctx, sender_account.account_id, domain_id)
        .ok_or_else(|| "No subscription exists for that domain.".to_string())?;

    subscription.active = false;
    subscription.updated_at = ctx.timestamp;
    ctx.db.subscription().subscription_id().update(subscription);
    Ok(())
}

fn ensure_sender_has_no_active_account(ctx: &ReducerContext) -> Result<(), String> {
    if active_account_for_identity(ctx, ctx.sender()).is_some() {
        return Err("This identity is already bound to an active account.".to_string());
    }
    Ok(())
}

fn require_active_sender_account(ctx: &ReducerContext) -> Result<Account, String> {
    active_account_for_identity(ctx, ctx.sender())
        .ok_or_else(|| "Create an account before using this reducer.".to_string())
}

fn active_account_id_for_view_identity(ctx: &ViewContext, identity: Identity) -> Option<u64> {
    let key = ctx.db.account_key().key_identity().find(identity)?;
    if key.revoked_at.is_some() {
        return None;
    }

    let account = ctx.db.account().account_id().find(key.account_id)?;
    if account.status != AccountStatus::Active {
        return None;
    }

    Some(account.account_id)
}

fn ensure_named_domain_claim_allowed(ctx: &ReducerContext, account_id: u64) -> Result<(), String> {
    let cutoff_micros =
        ctx.timestamp.to_micros_since_unix_epoch() - DOMAIN_CLAIM_WINDOW_SECS * 1_000_000;
    let recent_claims = count_recent_named_domain_claims(ctx.db.domain().iter().filter(|domain| {
        domain.created_by_account_id == account_id
            && domain.created_at.to_micros_since_unix_epoch() >= cutoff_micros
    }));

    if recent_claims >= MAX_NAMED_DOMAIN_CLAIMS_PER_WINDOW {
        return Err(format!(
            "You may only create {} named domains every {} hours.",
            MAX_NAMED_DOMAIN_CLAIMS_PER_WINDOW,
            DOMAIN_CLAIM_WINDOW_SECS / 3_600
        ));
    }

    Ok(())
}

fn ensure_slug_is_available(ctx: &ReducerContext, slug: &str) -> Result<(), String> {
    if ctx
        .db
        .domain()
        .iter()
        .any(|domain| domain.kind != DomainKind::Dm && domain.slug == slug)
    {
        return Err("That domain slug has already been claimed.".to_string());
    }

    Ok(())
}

fn active_account_for_identity(ctx: &ReducerContext, identity: Identity) -> Option<Account> {
    let key = ctx.db.account_key().key_identity().find(identity)?;
    if key.revoked_at.is_some() {
        return None;
    }

    let account = ctx.db.account().account_id().find(key.account_id)?;
    if account.status != AccountStatus::Active {
        return None;
    }

    Some(account)
}

fn require_active_account(ctx: &ReducerContext, account_id: u64) -> Result<Account, String> {
    let account = ctx
        .db
        .account()
        .account_id()
        .find(account_id)
        .ok_or_else(|| "That account does not exist.".to_string())?;

    if account.status != AccountStatus::Active {
        return Err("That account is not active.".to_string());
    }

    Ok(account)
}

fn require_domain(ctx: &ReducerContext, domain_id: u64) -> Result<Domain, String> {
    ctx.db
        .domain()
        .domain_id()
        .find(domain_id)
        .ok_or_else(|| "That domain does not exist.".to_string())
}

fn require_domain_owner(
    ctx: &ReducerContext,
    domain_id: u64,
    account_id: u64,
) -> Result<(), String> {
    let membership = find_domain_membership(ctx, domain_id, account_id)
        .ok_or_else(|| "You are not a member of that domain.".to_string())?;

    if membership.role != DomainRole::Owner {
        return Err("Only domain owners can perform that action.".to_string());
    }

    Ok(())
}

fn can_read_domain(ctx: &ReducerContext, domain: &Domain, account_id: u64) -> bool {
    matches!(domain.kind, DomainKind::Public)
        || has_domain_membership(ctx, domain.domain_id, account_id)
}

fn can_post_in_domain(ctx: &ReducerContext, domain: &Domain, account_id: u64) -> bool {
    match domain.kind {
        DomainKind::Public => true,
        DomainKind::Private | DomainKind::Dm => {
            has_domain_membership(ctx, domain.domain_id, account_id)
        }
    }
}

fn has_domain_membership(ctx: &ReducerContext, domain_id: u64, account_id: u64) -> bool {
    find_domain_membership(ctx, domain_id, account_id).is_some()
}

fn find_domain_membership(
    ctx: &ReducerContext,
    domain_id: u64,
    account_id: u64,
) -> Option<DomainMember> {
    ctx.db
        .domain_member()
        .iter()
        .find(|membership| membership.domain_id == domain_id && membership.account_id == account_id)
}

fn insert_domain_member(
    ctx: &ReducerContext,
    domain_id: u64,
    account_id: u64,
    role: DomainRole,
) -> Result<(), String> {
    if has_domain_membership(ctx, domain_id, account_id) {
        return Err("That account is already a member of the domain.".to_string());
    }

    ctx.db.domain_member().insert(DomainMember {
        membership_id: 0,
        domain_id,
        account_id,
        role,
        joined_at: ctx.timestamp,
    });

    Ok(())
}

fn find_subscription(
    ctx: &ReducerContext,
    subscriber_account_id: u64,
    domain_id: u64,
) -> Option<Subscription> {
    ctx.db.subscription().iter().find(|subscription| {
        subscription.subscriber_account_id == subscriber_account_id
            && subscription.domain_id == domain_id
    })
}

fn next_domain_sequence(ctx: &ReducerContext, domain_id: u64) -> u64 {
    next_domain_sequence_from_existing(
        ctx.db
            .message()
            .messages_by_domain_id()
            .filter(domain_id)
            .map(|message| message.domain_sequence),
    )
}

fn next_domain_sequence_from_existing(existing_sequences: impl Iterator<Item = u64>) -> u64 {
    existing_sequences.max().unwrap_or(0) + 1
}

fn find_existing_dm(ctx: &ReducerContext, participant_account_ids: &[u64]) -> Option<Domain> {
    ctx.db.domain().iter().find(|domain| {
        if domain.kind != DomainKind::Dm {
            return false;
        }

        dm_member_account_ids(ctx, domain.domain_id) == participant_account_ids
    })
}

fn canonicalize_dm_participants(
    ctx: &ReducerContext,
    sender_account_id: u64,
    recipient_account_ids: Vec<u64>,
) -> Result<Vec<u64>, String> {
    let participant_account_ids =
        normalize_dm_participant_account_ids(sender_account_id, recipient_account_ids);
    if participant_account_ids.len() < 2 {
        return Err("DM creation requires at least one other active account.".to_string());
    }

    for account_id in participant_account_ids.iter().copied() {
        if account_id == sender_account_id {
            continue;
        }
        require_active_account(ctx, account_id)?;
    }

    Ok(participant_account_ids)
}

fn normalize_dm_participant_account_ids(
    sender_account_id: u64,
    recipient_account_ids: Vec<u64>,
) -> Vec<u64> {
    let mut participant_account_ids = vec![sender_account_id];
    for account_id in recipient_account_ids {
        if account_id == sender_account_id {
            continue;
        }
        if !participant_account_ids.contains(&account_id) {
            participant_account_ids.push(account_id);
        }
    }

    participant_account_ids.sort_unstable();
    participant_account_ids
}

fn count_recent_named_domain_claims(domains: impl Iterator<Item = Domain>) -> usize {
    domains
        .filter(|domain| domain.kind != DomainKind::Dm)
        .count()
}

fn dm_member_account_ids(ctx: &ReducerContext, domain_id: u64) -> Vec<u64> {
    let mut member_account_ids: Vec<u64> = ctx
        .db
        .domain_member()
        .iter()
        .filter(|membership| membership.domain_id == domain_id)
        .map(|membership| membership.account_id)
        .collect();
    member_account_ids.sort_unstable();
    member_account_ids
}

fn dm_member_account_ids_for_view(ctx: &ViewContext, domain_id: u64) -> Vec<u64> {
    let mut member_account_ids: Vec<u64> = ctx
        .db
        .domain_member()
        .domain_members_by_domain_id()
        .filter(domain_id)
        .map(|membership| membership.account_id)
        .collect();
    member_account_ids.sort_unstable();
    member_account_ids
}

fn normalize_message_char_limit(limit: u16) -> Result<u16, String> {
    if !(MIN_MESSAGE_CHAR_LIMIT..=MAX_MESSAGE_CHAR_LIMIT).contains(&limit) {
        return Err(format!(
            "Message char limit must be between {} and {}.",
            MIN_MESSAGE_CHAR_LIMIT, MAX_MESSAGE_CHAR_LIMIT
        ));
    }

    Ok(limit)
}

fn normalize_batch_window(batch_window_seconds: u32) -> Result<u32, String> {
    if batch_window_seconds == 0 {
        return Ok(DEFAULT_BATCH_WINDOW_SECONDS);
    }
    if batch_window_seconds > MAX_BATCH_WINDOW_SECONDS {
        return Err(format!(
            "Batch window must be {} seconds or less.",
            MAX_BATCH_WINDOW_SECONDS
        ));
    }

    Ok(batch_window_seconds)
}

fn validate_handle(ctx: &ReducerContext, handle: &str) -> Result<(), String> {
    let handle = handle.trim();
    if handle.is_empty() {
        return Err("Handle cannot be empty.".to_string());
    }
    if handle.len() > MAX_HANDLE_LEN {
        return Err(format!(
            "Handle must be at most {} characters.",
            MAX_HANDLE_LEN
        ));
    }
    if !handle
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(
            "Handle may only contain lowercase ASCII letters, numbers, dashes, and underscores."
                .to_string(),
        );
    }
    if ctx
        .db
        .account()
        .iter()
        .any(|account| account.handle == handle)
    {
        return Err("That handle is already taken.".to_string());
    }

    Ok(())
}

fn validate_domain_fields(
    kind: DomainKind,
    slug: &str,
    title: &str,
    message_char_limit: u16,
) -> Result<(), String> {
    validate_title(title)?;
    normalize_message_char_limit(message_char_limit)?;

    match kind {
        DomainKind::Public | DomainKind::Private => validate_slug(slug),
        DomainKind::Dm => {
            if !slug.trim().is_empty() {
                return Err("DM domains may not define a slug.".to_string());
            }
            Ok(())
        }
    }
}

fn validate_slug(slug: &str) -> Result<(), String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Err("Slug cannot be empty for public or private domains.".to_string());
    }
    if slug.len() > MAX_SLUG_LEN {
        return Err(format!("Slug must be at most {} characters.", MAX_SLUG_LEN));
    }
    if !slug
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
    {
        return Err(
            "Slug may only contain lowercase ASCII letters, numbers, dashes, and underscores."
                .to_string(),
        );
    }

    Ok(())
}

fn validate_title(title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() {
        return Err("Title cannot be empty.".to_string());
    }
    if title.len() > MAX_TITLE_LEN {
        return Err(format!(
            "Title must be at most {} characters.",
            MAX_TITLE_LEN
        ));
    }

    Ok(())
}

fn validate_message_body(body: &str, message_char_limit: usize) -> Result<(), String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("Message body cannot be empty.".to_string());
    }
    if body.chars().count() > message_char_limit {
        return Err(format!(
            "Message body must be at most {} characters.",
            message_char_limit
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_domain_sequence_starts_at_one() {
        assert_eq!(next_domain_sequence_from_existing([].into_iter()), 1);
    }

    #[test]
    fn next_domain_sequence_advances_from_max_seen_value() {
        assert_eq!(
            next_domain_sequence_from_existing([1, 2, 7, 3].into_iter()),
            8
        );
    }

    #[test]
    fn normalize_dm_participants_sorts_dedupes_and_includes_sender() {
        let participants = normalize_dm_participant_account_ids(10, vec![12, 10, 11, 12, 9]);
        assert_eq!(participants, vec![9, 10, 11, 12]);
    }

    #[test]
    fn normalize_dm_participants_can_collapse_to_sender_only() {
        let participants = normalize_dm_participant_account_ids(10, vec![10, 10]);
        assert_eq!(participants, vec![10]);
    }

    #[test]
    fn count_recent_named_domain_claims_ignores_dm_domains() {
        let domains = vec![
            Domain {
                domain_id: 1,
                kind: DomainKind::Public,
                slug: "alpha".to_string(),
                title: "Alpha".to_string(),
                created_by_account_id: 1,
                created_at: Timestamp::from_micros_since_unix_epoch(1),
                message_char_limit: DEFAULT_MESSAGE_CHAR_LIMIT,
            },
            Domain {
                domain_id: 2,
                kind: DomainKind::Dm,
                slug: String::new(),
                title: "DM".to_string(),
                created_by_account_id: 1,
                created_at: Timestamp::from_micros_since_unix_epoch(1),
                message_char_limit: DEFAULT_MESSAGE_CHAR_LIMIT,
            },
            Domain {
                domain_id: 3,
                kind: DomainKind::Private,
                slug: "beta".to_string(),
                title: "Beta".to_string(),
                created_by_account_id: 1,
                created_at: Timestamp::from_micros_since_unix_epoch(1),
                message_char_limit: DEFAULT_MESSAGE_CHAR_LIMIT,
            },
        ];

        assert_eq!(count_recent_named_domain_claims(domains.into_iter()), 2);
    }

    #[test]
    fn validate_message_body_rejects_blank_messages() {
        assert!(validate_message_body("   ", 10).is_err());
    }

    #[test]
    fn validate_message_body_enforces_character_limit() {
        assert!(validate_message_body("hello", 4).is_err());
        assert!(validate_message_body("hello", 5).is_ok());
    }

    #[test]
    fn normalize_batch_window_uses_default_for_zero() {
        assert_eq!(
            normalize_batch_window(0).expect("zero should map to default"),
            DEFAULT_BATCH_WINDOW_SECONDS
        );
    }
}
