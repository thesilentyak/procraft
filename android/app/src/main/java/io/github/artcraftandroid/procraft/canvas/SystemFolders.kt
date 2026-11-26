package io.github.artcraftandroid.procraft.canvas

import android.content.ContentResolver
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import android.provider.DocumentsContract.Document
import io.github.artcraftandroid.procraft.core.FolderAccess
import io.github.artcraftandroid.procraft.core.FolderEntry
import io.github.artcraftandroid.procraft.core.FolderException
import java.io.File

/**
 * The folders picked with the system's folder picker (a document tree, its permission kept), as the
 * core reaches them for plugins' folders: their files listed, a file written in whole, a file read
 * out whole. Nothing here takes a file of the person's away; what goes where is the core's
 * (crates/core folders.rs). A folder whose permission was taken back, or that is gone, is
 * [FolderException.Unreachable].
 */
class SystemFolders(context: Context) : FolderAccess {
    private val resolver: ContentResolver = context.applicationContext.contentResolver

    /** A file of the folder: its document, name, size and when it last changed. */
    private class Child(val uri: Uri, val name: String, val size: Long, val modified: Long, val flags: Int)

    override fun list(location: String): List<FolderEntry> =
        reaching(location) { tree -> children(tree).filter { listed(it.name) }.map { FolderEntry(it.name, it.size.coerceAtLeast(0).toULong(), it.modified) } }

    override fun writeWhole(location: String, name: String, from: String): FolderEntry = reaching(location) { tree ->
        if (!plainName(name)) throw FolderException.Failed("$name isn't a file's name")
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val before = children(tree)
        // what an earlier write cut short left: the app's own, hidden
        before.filter { it.name == partialName(name) || it.name == asideName(name) }.forEach { runCatching { DocumentsContract.deleteDocument(resolver, it.uri) } }
        val partial = DocumentsContract.createDocument(resolver, parent, "application/octet-stream", partialName(name))
            ?: throw FolderException.Failed("$name couldn't be begun")
        try {
            resolver.openFileDescriptor(partial, "w")?.use { fd ->
                java.io.FileOutputStream(fd.fileDescriptor).use { out ->
                    File(from).inputStream().use { it.copyTo(out, 1 shl 16) }
                    out.flush()
                    runCatching { out.fd.sync() }
                }
            } ?: throw FolderException.Failed("$name couldn't be written")
        } catch (e: Exception) {
            runCatching { DocumentsContract.deleteDocument(resolver, partial) }
            throw failed(e)
        }
        val there = before.firstOrNull { it.name == name }
        val placed = putInPlace(partial, there, name)
        children(tree).firstOrNull { it.uri == placed || it.name == name }
            ?.let { FolderEntry(it.name, it.size.coerceAtLeast(0).toULong(), it.modified) }
            ?: throw FolderException.Failed("$name wasn't written")
    }

    override fun readWhole(location: String, name: String, to: String): Unit = reaching(location) { tree ->
        val child = children(tree).firstOrNull { it.name == name } ?: throw FolderException.Failed("$name isn't there")
        val target = File(to)
        val beside = File(target.parentFile, ".${target.name}.partial")
        try {
            (resolver.openInputStream(child.uri) ?: throw FolderException.Failed("$name couldn't be read")).use { input ->
                beside.outputStream().use { input.copyTo(it, 1 shl 16) }
            }
            if (!beside.renameTo(target)) {
                target.delete()
                if (!beside.renameTo(target)) throw FolderException.Failed("$name couldn't be kept")
            }
        } catch (e: Exception) {
            beside.delete()
            throw failed(e)
        }
    }

    /**
     * The whole file written at [partial] put where [there] (the file of that [name] there now, if
     * any) is: the old one moved aside, the new one named in its place, then the old one let go, so
     * there is a whole file under its name throughout but for a rename's moment. A provider that
     * can't rename has the new one copied over the old.
     */
    private fun putInPlace(partial: Uri, there: Child?, name: String): Uri {
        if (there != null && there.flags and Document.FLAG_SUPPORTS_RENAME == 0) {
            try {
                resolver.openOutputStream(there.uri, "wt")?.use { out -> resolver.openInputStream(partial)?.use { it.copyTo(out, 1 shl 16) } }
            } catch (e: Exception) {
                throw failed(e)
            } finally {
                runCatching { DocumentsContract.deleteDocument(resolver, partial) }
            }
            return there.uri
        }
        val aside = there?.let { DocumentsContract.renameDocument(resolver, it.uri, asideName(name)) ?: throw FolderException.Failed("$name couldn't be replaced") }
        val placed = runCatching { DocumentsContract.renameDocument(resolver, partial, name) }.getOrNull()
        if (placed == null) {
            // the old one back in its place, untouched
            aside?.let { runCatching { DocumentsContract.renameDocument(resolver, it, name) } }
            runCatching { DocumentsContract.deleteDocument(resolver, partial) }
            throw FolderException.Failed("$name couldn't be put in place")
        }
        aside?.let { runCatching { DocumentsContract.deleteDocument(resolver, it) } }
        return placed
    }

    /** The files directly in the folder [tree], its folders left out. */
    private fun children(tree: Uri): List<Child> {
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val columns = arrayOf(Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_SIZE, Document.COLUMN_LAST_MODIFIED, Document.COLUMN_MIME_TYPE, Document.COLUMN_FLAGS)
        val found = mutableListOf<Child>()
        (resolver.query(children, columns, null, null, null) ?: throw FolderException.Unreachable("the folder can't be read")).use { c ->
            while (c.moveToNext()) {
                val id = c.getString(0) ?: continue
                val name = c.getString(1) ?: continue
                if (c.getString(4) == Document.MIME_TYPE_DIR) continue
                found += Child(DocumentsContract.buildDocumentUriUsingTree(tree, id), name, if (c.isNull(2)) 0 else c.getLong(2), if (c.isNull(3)) 0 else c.getLong(3), if (c.isNull(5)) 0 else c.getInt(5))
            }
        }
        return found
    }

    /** [work] on the folder at [location] while the app may still reach it; a folder it can't is unreachable, anything else failed. */
    private fun <T> reaching(location: String, work: (Uri) -> T): T {
        val tree = runCatching { Uri.parse(location) }.getOrNull() ?: throw FolderException.Unreachable("$location isn't a folder")
        val kept = resolver.persistedUriPermissions.any { it.uri == tree && it.isReadPermission && it.isWritePermission }
        if (!kept) throw FolderException.Unreachable("the app's permission to the folder was taken back")
        return try {
            work(tree)
        } catch (e: FolderException) {
            throw e
        } catch (e: SecurityException) {
            throw FolderException.Unreachable(e.message ?: "the folder's permission")
        } catch (e: IllegalArgumentException) {
            throw FolderException.Unreachable(e.message ?: "the folder is gone")
        } catch (e: java.io.FileNotFoundException) {
            throw FolderException.Unreachable(e.message ?: "the folder is gone")
        } catch (e: Exception) {
            throw failed(e)
        }
    }

    private fun failed(e: Exception): FolderException = e as? FolderException ?: FolderException.Failed(e.message ?: e.javaClass.simpleName)

    companion object {
        /** The permission the app keeps to a folder picked with the system's picker. */
        const val KEPT = Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION

        /** Where a file named [name] is written before it is whole: hidden, beside it. */
        fun partialName(name: String) = ".$name.partial"

        /** Where the file named [name] waits while its new one takes its name: hidden, beside it. */
        fun asideName(name: String) = ".$name.replaced"

        /** Whether a file named [name] is listed: hidden files (an app's half-written ones among them) aren't. */
        fun listed(name: String) = name.isNotEmpty() && !name.startsWith(".")

        /** Whether [name] is one file's name in a folder: nothing that reaches out of it. */
        fun plainName(name: String) = name.isNotEmpty() && name != "." && name != ".." && name.none { it == '/' || it == '\\' || it == '\u0000' }

        /** The name a picked folder shows by: its own, as the system gives it. */
        fun shownName(context: Context, tree: Uri): String? = runCatching {
            val doc = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
            context.contentResolver.query(doc, arrayOf(Document.COLUMN_DISPLAY_NAME), null, null, null)?.use { c ->
                if (c.moveToFirst()) c.getString(0) else null
            }
        }.getOrNull()
    }
}
