package dev.isxander.nand2tetris

import com.intellij.openapi.components.service
import com.intellij.openapi.options.Configurable
import com.intellij.openapi.project.Project
import com.intellij.ui.dsl.builder.panel
import com.redhat.devtools.lsp4ij.LanguageServerManager
import javax.swing.JComponent
import javax.swing.JTextField

class N2tConfigurable(private val project: Project) : Configurable {
    private var executableField: JTextField? = null

    override fun getDisplayName(): String = "Nand2Tetris"

    override fun createComponent(): JComponent = panel {
        row("n2t executable:") {
            executableField = textField()
                .comment("Leave empty to use PATH or automatically download the latest GitHub release.")
                .component
        }
    }.also { reset() }

    override fun isModified(): Boolean = executableField?.let {
        it.text.trim() != project.service<N2tSettings>().getState().executablePath
    } ?: false

    override fun apply() {
        val field = executableField ?: return
        if (!isModified()) return
        project.service<N2tSettings>().getState().executablePath = field.text.trim()
        LanguageServerManager.getInstance(project).start("n2t")
    }

    override fun reset() {
        executableField?.text = project.service<N2tSettings>().getState().executablePath
    }

    override fun disposeUIResources() {
        executableField = null
    }
}
