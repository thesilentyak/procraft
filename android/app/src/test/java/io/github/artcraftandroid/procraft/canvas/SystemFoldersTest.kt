package io.github.artcraftandroid.procraft.canvas

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SystemFoldersTest {
    @Test
    fun aFileBeingWrittenOrReplacedIsHiddenBesideItsName() {
        assertEquals(".Dragon.procraft.partial", SystemFolders.partialName("Dragon.procraft"))
        assertEquals(".Dragon.procraft.replaced", SystemFolders.asideName("Dragon.procraft"))
        // hidden: never listed, so never taken for a package or brought in
        assertFalse(SystemFolders.listed(SystemFolders.partialName("Dragon.procraft")))
        assertFalse(SystemFolders.listed(SystemFolders.asideName("Dragon.procraft")))
        assertFalse(SystemFolders.listed(".stfolder"))
        assertFalse(SystemFolders.listed(""))
        assertTrue(SystemFolders.listed("Dragon.procraft"))
        assertTrue(SystemFolders.listed("Dragon time-lapse.mp4"))
    }

    @Test
    fun onlyOneFilesNameIsWritten() {
        for (name in listOf("Dragon.procraft", "Dragon_2.procraft", "Dragon time-lapse 2.mp4", "a.b.c")) {
            assertTrue(name, SystemFolders.plainName(name))
        }
        for (name in listOf("", ".", "..", "../up.procraft", "a/b.procraft", "a\\b.procraft", "nul\u0000.procraft")) {
            assertFalse(name, SystemFolders.plainName(name))
        }
    }

    @Test
    fun thePermissionKeptIsToReadAndToWrite() {
        assertEquals(
            android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION or android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            SystemFolders.KEPT,
        )
    }
}
