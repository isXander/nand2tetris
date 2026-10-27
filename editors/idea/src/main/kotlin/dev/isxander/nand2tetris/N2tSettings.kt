package dev.isxander.nand2tetris

import com.intellij.openapi.components.PersistentStateComponent
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.components.StoragePathMacros

@Service(Service.Level.PROJECT)
@State(name = "Nand2Tetris", storages = [Storage(StoragePathMacros.WORKSPACE_FILE)])
class N2tSettings : PersistentStateComponent<N2tSettings.Options> {
    data class Options(var executablePath: String = "")

    private var options = Options()

    override fun getState(): Options = options

    override fun loadState(state: Options) {
        options = state
    }
}
