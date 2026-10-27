package dev.isxander.nand2tetris

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.components.service
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.server.CannotStartProcessException
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import java.io.IOException

class N2tLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider =
        object : OSProcessStreamConnectionProvider() {
            // LSP4IJ starts connections in the background; don't download in the factory/UI.
            override fun start() {
                try {
                    val executable = service<N2tServerBinary>().resolve(
                        project.service<N2tSettings>().getState().executablePath
                    )
                    val command = GeneralCommandLine(executable, "lsp")
                        .withParentEnvironmentType(GeneralCommandLine.ParentEnvironmentType.CONSOLE)
                    project.basePath?.let { command.withWorkDirectory(it) }
                    setCommandLine(command)
                } catch (error: IOException) {
                    throw CannotStartProcessException(error)
                }
                super.start()
            }
        }
}
