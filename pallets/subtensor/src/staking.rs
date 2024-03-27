use super::*;
use frame_support::storage::IterableStorageDoubleMap;
use frame_support::storage::IterableStorageNMap;

impl<T: Config> Pallet<T> {
    // ---- The implementation for the extrinsic become_delegate: signals that this hotkey allows delegated stake.
    //
    // # Args:
    // 	* 'origin': (<T as frame_system::Config>RuntimeOrigin):
    // 		- The signature of the caller's coldkey.
    //
    // 	* 'hotkey' (T::AccountId):
    // 		- The hotkey we are delegating (must be owned by the coldkey.)
    //
    // 	* 'take' (u16):
    // 		- The stake proportion that this hotkey takes from delegations.
    //
    // # Event:
    // 	* DelegateAdded;
    // 		- On successfully setting a hotkey as a delegate.
    //
    // # Raises:
    // 	* 'NotRegistered':
    // 		- The hotkey we are delegating is not registered on the network.
    //
    // 	* 'NonAssociatedColdKey':
    // 		- The hotkey we are delegating is not owned by the calling coldket.
    //
    // 	* 'TxRateLimitExceeded':
    // 		- Thrown if key has hit transaction rate limit
    //
    pub fn do_become_delegate(
        origin: T::RuntimeOrigin,
        hotkey: T::AccountId,
        take: u16,
    ) -> dispatch::DispatchResult {
        // --- 1. We check the coldkey signuture.
        let coldkey = ensure_signed(origin)?;
        log::info!(
            "do_become_delegate( origin:{:?} hotkey:{:?}, take:{:?} )",
            coldkey,
            hotkey,
            take
        );

        // --- 2. Ensure we are delegating an known key.
        ensure!(
            Self::hotkey_account_exists(&hotkey),
            Error::<T>::NotRegistered
        );

        // --- 3. Ensure that the coldkey is the owner.
        ensure!(
            Self::coldkey_owns_hotkey(&coldkey, &hotkey),
            Error::<T>::NonAssociatedColdKey
        );

        // --- 4. Ensure we are not already a delegate (dont allow changing delegate take.)
        ensure!(
            !Self::hotkey_is_delegate(&hotkey),
            Error::<T>::AlreadyDelegate
        );

        // --- 5. Ensure we don't exceed tx rate limit
        let block: u64 = Self::get_current_block_as_u64();
        ensure!(
            !Self::exceeds_tx_rate_limit(Self::get_last_tx_block(&coldkey), block),
            Error::<T>::TxRateLimitExceeded
        );

        // --- 6. Delegate the key.
        Self::delegate_hotkey(&hotkey, take);

        // Set last block for rate limiting
        Self::set_last_tx_block(&coldkey, block);

        // --- 7. Emit the staking event.
        log::info!(
            "DelegateAdded( coldkey:{:?}, hotkey:{:?}, take:{:?} )",
            coldkey,
            hotkey,
            take
        );
        Self::deposit_event(Event::DelegateAdded(coldkey, hotkey, take));

        // --- 8. Ok and return.
        Ok(())
    }

    // ---- The implementation for the extrinsic add_stake: Adds stake to a hotkey account.
    //
    // # Args:
    // 	* 'origin': (<T as frame_system::Config>RuntimeOrigin):
    // 		- The signature of the caller's coldkey.
    //
    // 	* 'hotkey' (T::AccountId):
    // 		- The associated hotkey account.
    //
    // 	* 'netuid' (u16):
    // 		- The netuid to stake into.
    //
    // 	* 'stake_to_be_added' (u64):
    // 		- The amount of stake to be added to the hotkey staking account.
    //
    // # Event:
    // 	* StakeAdded;
    // 		- On the successfully adding stake to a global account.
    //
    // # Raises:
    // 	* 'CouldNotConvertToBalance':
    // 		- Unable to convert the passed stake value to a balance.
    //
    // 	* 'NotEnoughBalanceToStake':
    // 		- Not enough balance on the coldkey to add onto the global account.
    //
    // 	* 'NonAssociatedColdKey':
    // 		- The calling coldkey is not associated with this hotkey.
    //
    // 	* 'BalanceWithdrawalError':
    // 		- Errors stemming from transaction pallet.
    //
    // 	* 'TxRateLimitExceeded':
    // 		- Thrown if key has hit transaction rate limit
    //
    pub fn do_add_stake(
        origin: T::RuntimeOrigin,
        hotkey: T::AccountId,
        netuid: u16,
        stake_to_be_added: u64,
    ) -> dispatch::DispatchResult {
        // --- 1. We check that the transaction is signed by the caller and retrieve the T::AccountId coldkey information.
        let coldkey = ensure_signed(origin)?;
        log::info!(
            "do_add_stake( origin:{:?} hotkey:{:?}, netuid:{:?}, stake_to_be_added:{:?} )",
            coldkey,
            hotkey,
            netuid,
            stake_to_be_added
        );

        // --- 2. Ensure that the netuid exists.
        ensure!(
            Self::if_subnet_exist(netuid),
            Error::<T>::NetworkDoesNotExist
        );

        // --- 3. We convert the stake u64 into a balance.
        let stake_as_balance = Self::u64_to_balance(stake_to_be_added);
        ensure!(
            stake_as_balance.is_some(),
            Error::<T>::CouldNotConvertToBalance
        );

        // --- 4. Ensure the callers coldkey has enough stake to perform the transaction.
        ensure!(
            Self::can_remove_balance_from_coldkey_account(&coldkey, stake_as_balance.unwrap()),
            Error::<T>::NotEnoughBalanceToStake
        );

        // --- 5. Ensure that the hotkey account exists this is only possible through registration.
        ensure!(
            Self::hotkey_account_exists(&hotkey),
            Error::<T>::NotRegistered
        );

        // --- 6. Ensure that the hotkey allows delegation or that the hotkey is owned by the calling coldkey.
        ensure!(
            Self::hotkey_is_delegate(&hotkey) || Self::coldkey_owns_hotkey(&coldkey, &hotkey),
            Error::<T>::NonAssociatedColdKey
        );

        // --- 7. Ensure we don't exceed tx rate limit
        let block: u64 = Self::get_current_block_as_u64();
        ensure!(
            !Self::exceeds_tx_rate_limit(Self::get_last_tx_block(&coldkey), block),
            Error::<T>::TxRateLimitExceeded
        );

        // --- 8. Ensure the remove operation from the coldkey is a success.
        ensure!(
            Self::remove_balance_from_coldkey_account(&coldkey, stake_as_balance.unwrap()) == true,
            Error::<T>::BalanceWithdrawalError
        );

        // --- 9. If we reach here, add the balance to the hotkey.
        Self::increase_stake_on_coldkey_hotkey_account(
            &coldkey,
            &hotkey,
            netuid,
            stake_to_be_added,
        );

        // Set last block for rate limiting
        Self::set_last_tx_block(&coldkey, block);

        // --- 10. Emit the staking event.
        log::info!(
            "StakeAdded( hotkey:{:?}, netuid:{:?}, stake_to_be_added:{:?} )",
            hotkey,
            netuid,
            stake_to_be_added
        );
        Self::deposit_event(Event::StakeAdded(hotkey, netuid, stake_to_be_added));

        // --- 10. Ok and return.
        Ok(())
    }

    // ---- The implementation for the extrinsic remove_stake: Removes stake from a hotkey account and adds it onto a coldkey.
    //
    // # Args:
    // 	* 'origin': (<T as frame_system::Config>RuntimeOrigin):
    // 		- The signature of the caller's coldkey.
    //
    // 	* 'hotkey' (T::AccountId):
    // 		- The associated hotkey account.
    //
    // 	* 'netuid' (u16):
    // 		- The netuid to remove stake from.
    //
    // 	* 'stake_to_be_added' (u64):
    // 		- The amount of stake to be added to the hotkey staking account.
    //
    // # Event:
    // 	* StakeRemoved;
    // 		- On the successfully removing stake from the hotkey account.
    //
    // # Raises:
    //
    //  * 'NetworkDoesNotExist':
    //      - Thrown if the subnet we are attempting to stake into does not exist.
    //
    // 	* 'NotRegistered':
    // 		- Thrown if the account we are attempting to unstake from is non existent.
    //
    // 	* 'NonAssociatedColdKey':
    // 		- Thrown if the coldkey does not own the hotkey we are unstaking from.
    //
    // 	* 'NotEnoughStaketoWithdraw':
    // 		- Thrown if there is not enough stake on the hotkey to withdwraw this amount.
    //
    // 	* 'CouldNotConvertToBalance':
    // 		- Thrown if we could not convert this amount to a balance.
    //
    // 	* 'TxRateLimitExceeded':
    // 		- Thrown if key has hit transaction rate limit
    //
    //
    pub fn do_remove_stake(
        origin: T::RuntimeOrigin,
        hotkey: T::AccountId,
        netuid: u16,
        stake_to_be_removed: u64,
    ) -> dispatch::DispatchResult {
        // --- 1. We check the transaction is signed by the caller and retrieve the T::AccountId coldkey information.
        let coldkey = ensure_signed(origin)?;
        log::info!(
            "do_remove_stake( origin:{:?} netuid:{:?}, hotkey:{:?}, stake_to_be_removed:{:?} )",
            coldkey,
            hotkey,
            netuid,
            stake_to_be_removed
        );

        // --- 2. Ensure that the netuid exists.
        ensure!(
            Self::if_subnet_exist(netuid),
            Error::<T>::NetworkDoesNotExist
        );

        // --- 3. Ensure that the hotkey account exists this is only possible through registration.
        ensure!(
            Self::hotkey_account_exists(&hotkey),
            Error::<T>::NotRegistered
        );

        // --- 4. Ensure that the hotkey allows delegation or that the hotkey is owned by the calling coldkey.
        ensure!(
            Self::hotkey_is_delegate(&hotkey) || Self::coldkey_owns_hotkey(&coldkey, &hotkey),
            Error::<T>::NonAssociatedColdKey
        );

        // --- 5. Ensure that the stake amount to be removed is above zero.
        ensure!(
            stake_to_be_removed > 0,
            Error::<T>::NotEnoughStaketoWithdraw
        );

        // --- 6. Ensure that the hotkey has enough stake to withdraw.
        ensure!(
            Self::has_enough_stake(&coldkey, &hotkey, netuid, stake_to_be_removed),
            Error::<T>::NotEnoughStaketoWithdraw
        );

        // --- 5. Ensure that we can conver this u64 to a balance.
        let stake_to_be_added_as_currency = Self::u64_to_balance(stake_to_be_removed);
        ensure!(
            stake_to_be_added_as_currency.is_some(),
            Error::<T>::CouldNotConvertToBalance
        );

        // --- 6. Ensure we don't exceed tx rate limit
        let block: u64 = Self::get_current_block_as_u64();
        ensure!(
            !Self::exceeds_tx_rate_limit(Self::get_last_tx_block(&coldkey), block),
            Error::<T>::TxRateLimitExceeded
        );

        // --- 7. We remove the balance from the hotkey.
        Self::decrease_stake_on_coldkey_hotkey_account(
            &coldkey,
            &hotkey,
            netuid,
            stake_to_be_removed,
        );

        // --- 8. We add the balancer to the coldkey.  If the above fails we will not credit this coldkey.
        Self::add_balance_to_coldkey_account(&coldkey, stake_to_be_added_as_currency.unwrap());

        // Set last block for rate limiting
        Self::set_last_tx_block(&coldkey, block);

        // --- 9. Emit the unstaking event.
        log::info!(
            "StakeRemoved( hotkey:{:?}, stake_to_be_removed:{:?} )",
            hotkey,
            stake_to_be_removed
        );
        Self::deposit_event(Event::StakeRemoved(hotkey, netuid, stake_to_be_removed));

        // --- 10. Done and ok.
        Ok(())
    }

    // Returns true if the passed hotkey allow delegative staking.
    //
    pub fn hotkey_is_delegate(hotkey: &T::AccountId) -> bool {
        return Delegates::<T>::contains_key(hotkey);
    }

    // Sets the hotkey as a delegate with take.
    //
    pub fn delegate_hotkey(hotkey: &T::AccountId, take: u16) {
        Delegates::<T>::insert(hotkey, take);
    }

    // Returns the total amount of stake in the staking table.
    //
    pub fn get_total_stake() -> u64 {
        return TotalStake::<T>::get();
    }

    // Returns the total amount of stake under a subnet (delegative or otherwise)
    pub fn get_total_stake_for_subnet(target_subnet: u16) -> u64 {
        SubStake::<T>::iter()
            .filter(|((_, _, subnet), _)| *subnet == target_subnet)
            .fold(0, |acc, (_, stake)| acc.saturating_add(stake))
    }

    // Increases the total amount of stake by the passed amount.
    //
    pub fn increase_total_stake(increment: u64) {
        TotalStake::<T>::put(Self::get_total_stake().saturating_add(increment));
    }

    // Decreases the total amount of stake by the passed amount.
    //
    pub fn decrease_total_stake(decrement: u64) {
        TotalStake::<T>::put(Self::get_total_stake().saturating_sub(decrement));
    }

    // Returns the total amount of stake under a hotkey (delegative or otherwise)
    //
    pub fn get_total_stake_for_hotkey(hotkey: &T::AccountId) -> u64 {
        return TotalHotkeyStake::<T>::get(hotkey);
    }

    // Returns the total amount of stake under a hotkey for a subnet (delegative or otherwise)
    //
    pub fn get_total_stake_for_hotkey_and_subnet(hotkey: &T::AccountId, netuid: u16) -> u64 {
        return TotalHotkeySubStake::<T>::get(hotkey, netuid);
    }

    // Returns the total amount of stake held by the coldkey (delegative or otherwise)
    //
    pub fn get_total_stake_for_coldkey(coldkey: &T::AccountId) -> u64 {
        return TotalColdkeyStake::<T>::get(coldkey);
    }

    // Creates a cold - hot pairing account if the hotkey is not already an active account.
    //
    pub fn create_account_if_non_existent(
        coldkey: &T::AccountId,
        hotkey: &T::AccountId,
        netuid: u16,
    ) {
        if !Self::hotkey_account_exists(hotkey) {
            Stake::<T>::insert(hotkey, coldkey, 0);
            SubStake::<T>::insert((hotkey, coldkey, netuid), 0);
            Owner::<T>::insert(hotkey, coldkey);
        }
    }

    // Returns the coldkey owning this hotkey. This function should only be called for active accounts.
    //
    pub fn get_owning_coldkey_for_hotkey(hotkey: &T::AccountId) -> T::AccountId {
        return Owner::<T>::get(hotkey);
    }

    // Returns true if the hotkey account has been created.
    //
    pub fn hotkey_account_exists(hotkey: &T::AccountId) -> bool {
        return Owner::<T>::contains_key(hotkey);
    }

    // Return true if the passed coldkey owns the hotkey.
    //
    pub fn coldkey_owns_hotkey(coldkey: &T::AccountId, hotkey: &T::AccountId) -> bool {
        if Self::hotkey_account_exists(hotkey) {
            return Owner::<T>::get(hotkey) == *coldkey;
        } else {
            return false;
        }
    }

    // Returns true if the cold-hot staking account has enough balance to fufil the decrement.
    //
    pub fn has_enough_stake(
        coldkey: &T::AccountId,
        hotkey: &T::AccountId,
        netuid: u16,
        decrement: u64,
    ) -> bool {
        return Self::get_stake_for_coldkey_and_hotkey(coldkey, hotkey, netuid) >= decrement;
    }

    // Increases the stake on the hotkey account under its owning coldkey.
    //
    pub fn increase_stake_on_hotkey_account(hotkey: &T::AccountId, netuid: u16, increment: u64) {
        Self::increase_stake_on_coldkey_hotkey_account(
            &Self::get_owning_coldkey_for_hotkey(hotkey),
            hotkey,
            netuid,
            increment,
        );
    }

    // Decreases the stake on the hotkey account under its owning coldkey.
    //
    pub fn decrease_stake_on_hotkey_account(hotkey: &T::AccountId, netuid: u16, decrement: u64) {
        Self::decrease_stake_on_coldkey_hotkey_account(
            &Self::get_owning_coldkey_for_hotkey(hotkey),
            hotkey,
            netuid,
            decrement,
        );
    }

    // Returns the stake under the cold - hot pairing in the staking table.
    //
    pub fn get_stake_for_coldkey_and_hotkey(
        coldkey: &T::AccountId,
        hotkey: &T::AccountId,
        netuid: u16,
    ) -> u64 {
        SubStake::<T>::try_get((hotkey, coldkey, netuid)).unwrap_or(0)
    }

    // Increases the stake on the cold - hot pairing by increment while also incrementing other counters.
    // This function should be called rather than set_stake under account.
    //
    pub fn increase_stake_on_coldkey_hotkey_account(
        coldkey: &T::AccountId,
        hotkey: &T::AccountId,
        netuid: u16,
        increment: u64,
    ) {
        if increment == 0 {
            return;
        }
        TotalColdkeyStake::<T>::insert(
            coldkey,
            TotalColdkeyStake::<T>::get(coldkey).saturating_add(increment),
        );
        TotalHotkeyStake::<T>::insert(
            hotkey,
            TotalHotkeyStake::<T>::get(hotkey).saturating_add(increment),
        );
        TotalHotkeySubStake::<T>::insert(
            hotkey,
            netuid,
            TotalHotkeySubStake::<T>::get(hotkey, netuid).saturating_add(increment),
        );
        Stake::<T>::insert(
            hotkey,
            coldkey,
            Stake::<T>::get(hotkey, coldkey).saturating_add(increment),
        );
        SubStake::<T>::insert(
            (hotkey, coldkey, netuid),
            SubStake::<T>::try_get((hotkey, coldkey, netuid))
                .unwrap_or(0)
                .saturating_add(increment),
        );
        TotalStake::<T>::put(TotalStake::<T>::get().saturating_add(increment));
    }

    // Decreases the stake on the cold - hot pairing by the decrement while decreasing other counters.
    //
    pub fn decrease_stake_on_coldkey_hotkey_account(
        coldkey: &T::AccountId,
        hotkey: &T::AccountId,
        netuid: u16,
        decrement: u64,
    ) {
        if decrement == 0 {
            return;
        }
        TotalColdkeyStake::<T>::insert(
            coldkey,
            TotalColdkeyStake::<T>::get(coldkey).saturating_sub(decrement),
        );
        TotalHotkeyStake::<T>::insert(
            hotkey,
            TotalHotkeyStake::<T>::get(hotkey).saturating_sub(decrement),
        );
        TotalHotkeySubStake::<T>::insert(
            hotkey,
            netuid,
            TotalHotkeySubStake::<T>::get(hotkey, netuid).saturating_sub(decrement),
        );
        Stake::<T>::insert(
            hotkey,
            coldkey,
            Stake::<T>::get(hotkey, coldkey).saturating_sub(decrement),
        );
        SubStake::<T>::insert(
            (hotkey, coldkey, netuid),
            SubStake::<T>::try_get((hotkey, coldkey, netuid))
                .unwrap_or(0)
                .saturating_sub(decrement),
        );
        TotalStake::<T>::put(TotalStake::<T>::get().saturating_sub(decrement));
    }

    pub fn u64_to_balance(
        input: u64,
    ) -> Option<
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance,
    > {
        input.try_into().ok()
    }

    pub fn add_balance_to_coldkey_account(
        coldkey: &T::AccountId,
        amount: <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance,
    ) {
        T::Currency::deposit_creating(&coldkey, amount); // Infallibe
    }

    pub fn set_balance_on_coldkey_account(
        coldkey: &T::AccountId,
        amount: <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance,
    ) {
        T::Currency::make_free_balance_be(&coldkey, amount);
    }

    pub fn can_remove_balance_from_coldkey_account(
        coldkey: &T::AccountId,
        amount: <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance,
    ) -> bool {
        let current_balance = Self::get_coldkey_balance(coldkey);
        if amount > current_balance {
            return false;
        }

        // This bit is currently untested. @todo
        let new_potential_balance = current_balance - amount;
        let can_withdraw = T::Currency::ensure_can_withdraw(
            &coldkey,
            amount,
            WithdrawReasons::except(WithdrawReasons::TIP),
            new_potential_balance,
        )
        .is_ok();
        can_withdraw
    }

    pub fn get_coldkey_balance(
        coldkey: &T::AccountId,
    ) -> <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance {
        return T::Currency::free_balance(&coldkey);
    }

    pub fn remove_balance_from_coldkey_account(
        coldkey: &T::AccountId,
        amount: <<T as Config>::Currency as Currency<<T as system::Config>::AccountId>>::Balance,
    ) -> bool {
        return match T::Currency::withdraw(
            &coldkey,
            amount,
            WithdrawReasons::except(WithdrawReasons::TIP),
            ExistenceRequirement::KeepAlive,
        ) {
            Ok(_result) => true,
            Err(_error) => false,
        };
    }

    pub fn unstake_all_coldkeys_from_hotkey_account(hotkey: &T::AccountId) {
        // Iterate through all coldkeys that have a stake on this hotkey account.
        for (coldkey_i, _) in
            <Stake<T> as IterableStorageDoubleMap<T::AccountId, T::AccountId, u64>>::iter_prefix(
                hotkey,
            )
        {
            for netuid in 0..(TotalNetworks::<T>::get() + 1) {
                // Get the stake on this uid.
                let stake_i = Self::get_stake_for_coldkey_and_hotkey(&coldkey_i, hotkey, netuid);

                // Convert to balance and add to the coldkey account.
                let stake_i_as_balance = Self::u64_to_balance(stake_i);
                if stake_i_as_balance.is_none() {
                    continue; // Don't unstake if we can't convert to balance.
                } else {
                    // Stake is successfully converted to balance.

                    // Remove the stake from the coldkey - hotkey pairing.
                    Self::decrease_stake_on_coldkey_hotkey_account(
                        &coldkey_i, hotkey, netuid, stake_i,
                    );

                    // Add the balance to the coldkey account.
                    Self::add_balance_to_coldkey_account(&coldkey_i, stake_i_as_balance.unwrap());
                }
            }
        }
    }
    
}
