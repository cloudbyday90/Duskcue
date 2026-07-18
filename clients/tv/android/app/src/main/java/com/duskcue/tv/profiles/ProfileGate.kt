package com.duskcue.tv.profiles

data class ProfileGateState(
    val profileSelectionRequired: Boolean,
    val parentUnlockRequired: Boolean = false,
)

fun ProfileGateState.canLoadProfileScopedContent(): Boolean = !profileSelectionRequired
