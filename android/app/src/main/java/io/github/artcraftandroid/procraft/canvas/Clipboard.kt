package io.github.artcraftandroid.procraft.canvas

/**
 * Whether Paste takes the picture on the system's clipboard, put there at [systemAt] (the wall
 * clock, as the system keeps it), over the copy made here ([ownCopy], made at [ownAt]): only one
 * put there after that copy, and after the copy itself went there ([ownPutAt], the time the system
 * gave it then, if it took it), and not the copy's own picture ([ours]). A system that puts a copy
 * of its own in place of ours as it takes it (a clipboard of its own) still pastes ours.
 */
internal fun pastesSystemPicture(ours: Boolean, systemAt: Long, ownCopy: Boolean, ownAt: Long, ownPutAt: Long?): Boolean =
    !ours && (!ownCopy || systemAt > maxOf(ownAt, ownPutAt ?: Long.MIN_VALUE))
