package dev.isxander.nand2tetris

import com.google.gson.JsonParser
import com.intellij.execution.configurations.PathEnvironmentVariableUtil
import com.intellij.openapi.application.PathManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.diagnostic.Logger
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.util.SystemInfo
import com.intellij.openapi.util.io.FileUtil
import com.intellij.util.io.Decompressor
import com.intellij.util.io.HttpRequests
import java.io.IOException
import java.nio.file.Files
import java.nio.file.LinkOption.NOFOLLOW_LINKS
import java.nio.file.Path
import java.nio.file.StandardCopyOption.REPLACE_EXISTING

/** Shared across projects so simultaneous server starts do not download twice. */
@Service(Service.Level.APP)
class N2tServerBinary {
    private var downloadedBinary: Path? = null

    @Synchronized
    fun resolve(configuredPath: String): String {
        configuredPath.trim().takeIf { it.isNotEmpty() }?.let { return it }
        PathEnvironmentVariableUtil.findInPath(binaryName)?.let { return it.absolutePath }
        downloadedBinary?.takeIf(::isUsable)?.let { return it.toString() }

        val cache = PathManager.getSystemDir().resolve("nand2tetris/servers/$target")
        val cached = cachedBinary(cache)
        val binary = try {
            downloadLatest(cache)
        } catch (error: IOException) {
            if (cached == null) {
                throw IOException(
                    "Could not find n2t on PATH or download a GitHub release: ${error.message}. " +
                        "Set the n2t executable in Settings | Languages & Frameworks | Nand2Tetris, " +
                        "or install it with cargo install --path crates/cli from the repository.",
                    error,
                )
            }
            Logger.getInstance(N2tServerBinary::class.java)
                .warn("Could not check/download the latest n2t release; using $cached", error)
            cached
        }
        downloadedBinary = binary
        return binary.toString()
    }

    private fun downloadLatest(cache: Path): Path {
        ProgressManager.checkCanceled()
        val indicator = ProgressManager.getInstance().progressIndicator
        indicator?.text = "Checking the latest Nand2Tetris release"
        val json = HttpRequests.request("https://api.github.com/repos/isXander/nand2tetris/releases/latest")
            .userAgent("nand2tetris-idea")
            .accept("application/vnd.github+json")
            .connectTimeout(15_000).readTimeout(30_000)
            .readString(indicator)
        val release = try {
            JsonParser.parseString(json).asJsonObject
        } catch (error: RuntimeException) {
            throw IOException("GitHub returned invalid release metadata", error)
        }
        val version = release.get("tag_name")?.asString
            ?.takeIf(::isVersionDirectory) ?: throw IOException("GitHub release has no valid tag")
        val extension = if (SystemInfo.isWindows) "zip" else "tar.gz"
        val assetName = "n2t-$target.$extension"
        val asset = release.getAsJsonArray("assets")?.firstOrNull {
            it.asJsonObject.get("name")?.asString == assetName
        }?.asJsonObject ?: throw IOException("Release $version has no asset $assetName")
        val downloadUrl = asset.get("browser_download_url")?.asString
            ?: throw IOException("Release asset $assetName has no download URL")
        val destination = cache.resolve(version).resolve(binaryName)
        if (isUsable(destination)) {
            Files.writeString(cache.resolve("current"), version)
            return destination
        }

        Files.createDirectories(cache)
        val staging = Files.createTempDirectory(cache, "download-")
        try {
            indicator?.text = "Downloading Nand2Tetris $version"
            val archive = staging.resolve(assetName)
            HttpRequests.request(downloadUrl).userAgent("nand2tetris-idea")
                .connectTimeout(15_000).readTimeout(60_000)
                .saveToFile(archive, indicator)
            ProgressManager.checkCanceled()
            val extracted = staging.resolve("extracted")
            val decompressor = if (SystemInfo.isWindows) Decompressor.Zip(archive) else Decompressor.Tar(archive)
            decompressor.filter { it.removePrefix("./") == binaryName }.extract(extracted)
            val executable = extracted.resolve(binaryName)
            if (!Files.isRegularFile(executable, NOFOLLOW_LINKS) || Files.size(executable) == 0L) {
                throw IOException("Archive $assetName does not contain $binaryName at its root")
            }
            if (!SystemInfo.isWindows && !executable.toFile().setExecutable(true, true)) {
                throw IOException("Could not make $binaryName executable")
            }
            ProgressManager.checkCanceled()
            Files.createDirectories(destination.parent)
            Files.move(executable, destination, REPLACE_EXISTING)
            Files.writeString(cache.resolve("current"), version)
            return destination
        } finally {
            FileUtil.delete(staging.toFile())
        }
    }

    private fun cachedBinary(cache: Path): Path? {
        return try {
            val version = Files.readString(cache.resolve("current")).trim()
            if (isVersionDirectory(version)) cache.resolve(version).resolve(binaryName).takeIf(::isUsable) else null
        } catch (_: IOException) {
            null
        }
    }

    private fun isUsable(path: Path): Boolean =
        Files.isRegularFile(path, NOFOLLOW_LINKS) && (SystemInfo.isWindows || Files.isExecutable(path))

    private fun isVersionDirectory(version: String): Boolean =
        version != "." && version != ".." && version.matches(Regex("[A-Za-z0-9._+-]+"))

    private val binaryName: String get() = if (SystemInfo.isWindows) "n2t.exe" else "n2t"

    private val target: String
        get() {
            val arch = when (SystemInfo.OS_ARCH.lowercase()) {
                "aarch64", "arm64" -> "aarch64"
                "x86_64", "amd64" -> "x86_64"
                else -> throw IOException("Unsupported Nand2Tetris architecture: ${SystemInfo.OS_ARCH}")
            }
            val os = when {
                SystemInfo.isMac -> "apple-darwin"
                SystemInfo.isLinux -> "unknown-linux-gnu"
                SystemInfo.isWindows -> "pc-windows-msvc"
                else -> throw IOException("Unsupported Nand2Tetris OS: ${SystemInfo.OS_NAME}")
            }
            return "$arch-$os"
        }
}
