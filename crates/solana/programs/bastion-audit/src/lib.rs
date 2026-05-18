use anchor_lang::prelude::*;

declare_id!("BaSZuLcwjfh75T3TjbVYpTH4qpJt1tNoZ3S6PTkvNhCb");

pub const AUDIT_SEED: &str = "bastion_audit";
pub const AGENT_SEED: &str = "bastion_agent";

#[program]
pub mod bastion_audit {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let audit_state = &mut ctx.accounts.audit_state;
        audit_state.owner = ctx.accounts.authority.key();
        audit_state.authority = ctx.accounts.authority.key();
        audit_state.bump = ctx.bumps.audit_state;
        audit_state.total_audits = 0;
        audit_state.allowed_count = 0;
        audit_state.blocked_count = 0;
        audit_state.paused = false;
        audit_state.paused_at = 0;
        audit_state.resumed_at = 0;
        Ok(())
    }

    pub fn log_audit(
        ctx: Context<LogAudit>,
        decision: u8,
        simulation_result: [u8; 32],
        reasoning: String,
        program_id: Option<[u8; 32]>,
    ) -> Result<()> {
        let audit_entry = &mut ctx.accounts.audit_entry;
        audit_entry.authority = ctx.accounts.signer.key();
        audit_entry.timestamp = Clock::get()?.unix_timestamp;
        audit_entry.decision = decision;
        audit_entry.simulation_result = simulation_result;
        audit_entry.reasoning = reasoning;
        audit_entry.program_id = program_id;
        audit_entry.bump = ctx.bumps.audit_entry;

        let audit_state = &mut ctx.accounts.audit_state;
        if decision == 0 {
            audit_state.allowed_count += 1;
        } else {
            audit_state.blocked_count += 1;
        }
        audit_state.total_audits += 1;

        Ok(())
    }

    pub fn register_agent(
        ctx: Context<RegisterAgent>,
        name: String,
        capability_bitmask: u64,
    ) -> Result<()> {
        let agent = &mut ctx.accounts.agent;
        agent.authority = ctx.accounts.signer.key();
        agent.name = name;
        agent.capability_bitmask = capability_bitmask;
        agent.reputation_score = 0;
        agent.registered_at = Clock::get()?.unix_timestamp;
        agent.bump = ctx.bumps.agent;

        emit!(AgentRegistered {
            agent: agent.key(),
            authority: agent.authority,
            name: agent.name.clone(),
        });

        Ok(())
    }

    pub fn update_agent_reputation(ctx: Context<UpdateReputation>, delta: i64) -> Result<()> {
        let agent = &mut ctx.accounts.agent;

        let new_score = agent.reputation_score as i64 + delta;
        require!(new_score >= 0, BastionError::InvalidReputation);

        agent.reputation_score = new_score as u64;

        emit!(ReputationUpdated {
            agent: agent.key(),
            new_score: agent.reputation_score,
        });

        Ok(())
    }

    pub fn set_policy(
        ctx: Context<SetPolicy>,
        allowed_programs: Vec<[u8; 32]>,
        max_sol_per_tx: u64,
        rate_limit_per_minute: u32,
    ) -> Result<()> {
        let policy = &mut ctx.accounts.policy;
        policy.authority = ctx.accounts.signer.key();
        policy.allowed_programs = allowed_programs;
        policy.max_sol_per_tx = max_sol_per_tx;
        policy.rate_limit_per_minute = rate_limit_per_minute;
        policy.bump = ctx.bumps.policy;

        Ok(())
    }

    pub fn emergency_pause(ctx: Context<EmergencyPause>) -> Result<()> {
        let audit_state = &mut ctx.accounts.audit_state;
        audit_state.paused = true;
        audit_state.paused_at = Clock::get()?.unix_timestamp;

        emit!(ProtocolPaused {
            authority: ctx.accounts.signer.key(),
        });

        Ok(())
    }

    pub fn emergency_resume(ctx: Context<EmergencyResume>) -> Result<()> {
        let audit_state = &mut ctx.accounts.audit_state;
        require!(audit_state.paused, BastionError::NotPaused);

        audit_state.paused = false;
        audit_state.resumed_at = Clock::get()?.unix_timestamp;

        emit!(ProtocolResumed {
            authority: ctx.accounts.signer.key(),
        });

        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        seeds = [AUDIT_SEED.as_bytes()],
        bump,
        payer = authority,
        space = 8 + std::mem::size_of::<AuditState>()
    )]
    pub audit_state: Account<'info, AuditState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LogAudit<'info> {
    #[account(
        init,
        seeds = [
            AUDIT_SEED.as_bytes(),
            &audit_state.total_audits.to_le_bytes()
        ],
        bump,
        payer = signer,
        space = 8 + std::mem::size_of::<AuditEntry>()
    )]
    pub audit_entry: Account<'info, AuditEntry>,
    #[account(
        mut,
        seeds = [AUDIT_SEED.as_bytes()],
        bump = audit_state.bump
    )]
    pub audit_state: Account<'info, AuditState>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterAgent<'info> {
    #[account(
        init,
        seeds = [AGENT_SEED.as_bytes(), signer.key().as_ref()],
        bump,
        payer = signer,
        space = 8 + std::mem::size_of::<Agent>()
    )]
    pub agent: Account<'info, Agent>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateReputation<'info> {
    #[account(
        mut,
        seeds = [AGENT_SEED.as_bytes(), agent.authority.as_ref()],
        bump = agent.bump
    )]
    pub agent: Account<'info, Agent>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct SetPolicy<'info> {
    #[account(
        init,
        seeds = [b"bastion_policy".as_ref()],
        bump,
        payer = signer,
        space = 8 + std::mem::size_of::<Policy>()
    )]
    pub policy: Account<'info, Policy>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EmergencyPause<'info> {
    #[account(
        mut,
        seeds = [AUDIT_SEED.as_bytes()],
        bump = audit_state.bump
    )]
    pub audit_state: Account<'info, AuditState>,
    pub signer: Signer<'info>,
}

#[derive(Accounts)]
pub struct EmergencyResume<'info> {
    #[account(
        mut,
        seeds = [AUDIT_SEED.as_bytes()],
        bump = audit_state.bump
    )]
    pub audit_state: Account<'info, AuditState>,
    pub signer: Signer<'info>,
}

#[account]
#[derive(Debug)]
pub struct AuditState {
    pub owner: Pubkey,
    pub authority: Pubkey,
    pub bump: u8,
    pub total_audits: u64,
    pub allowed_count: u64,
    pub blocked_count: u64,
    pub paused: bool,
    pub paused_at: i64,
    pub resumed_at: i64,
}

#[account]
#[derive(Debug)]
pub struct AuditEntry {
    pub authority: Pubkey,
    pub timestamp: i64,
    pub decision: u8,
    pub simulation_result: [u8; 32],
    pub reasoning: String,
    pub program_id: Option<[u8; 32]>,
    pub bump: u8,
}

#[account]
#[derive(Debug)]
pub struct Agent {
    pub authority: Pubkey,
    pub name: String,
    pub capability_bitmask: u64,
    pub reputation_score: u64,
    pub registered_at: i64,
    pub bump: u8,
}

#[account]
#[derive(Debug)]
pub struct Policy {
    pub authority: Pubkey,
    pub allowed_programs: Vec<[u8; 32]>,
    pub max_sol_per_tx: u64,
    pub rate_limit_per_minute: u32,
    pub bump: u8,
}

#[event]
pub struct AgentRegistered {
    pub agent: Pubkey,
    pub authority: Pubkey,
    pub name: String,
}

#[event]
pub struct ReputationUpdated {
    pub agent: Pubkey,
    pub new_score: u64,
}

#[event]
pub struct ProtocolPaused {
    pub authority: Pubkey,
}

#[event]
pub struct ProtocolResumed {
    pub authority: Pubkey,
}

#[error_code]
pub enum BastionError {
    #[msg("Invalid reputation score")]
    InvalidReputation,
    #[msg("Protocol is not paused")]
    NotPaused,
    #[msg("Protocol is paused")]
    IsPaused,
    #[msg("Unauthorized")]
    Unauthorized,
}
