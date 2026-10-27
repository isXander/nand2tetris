package dev.isxander.nand2tetris

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.extensions.PluginId
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider

class N2tTextMateBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
        val plugin = requireNotNull(
            PluginManagerCore.getPlugin(PluginId.getId("dev.isxander.nand2tetris"))
        )
        return listOf(
            TextMateBundleProvider.PluginBundle(
                "Nand2Tetris", plugin.pluginPath.resolve("textmate/nand2tetris")
            )
        )
    }
}
