package com.freemetar.app.data.sync

import android.app.Activity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.freemetar.app.data.link.FlightLogStore
import com.freemetar.app.domain.loc
import com.google.android.gms.auth.api.signin.GoogleSignInAccount
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.tasks.Task
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * ViewModel do zarządzania synchronizacją z Google Drive.
 */
class SyncViewModel(
    val driveSync: DriveSync,
    private val flightStore: FlightLogStore
) : ViewModel() {

    private val _syncState = MutableStateFlow<SyncState>(SyncState.Idle)
    val syncState: StateFlow<SyncState> = _syncState.asStateFlow()

    private val _isSignedIn = MutableStateFlow(false)
    val isSignedIn: StateFlow<Boolean> = _isSignedIn.asStateFlow()

    sealed class SyncState {
        data object Idle : SyncState()
        data object SigningIn : SyncState()
        data object Syncing : SyncState()
        data class Success(val message: String) : SyncState()
        data class Error(val message: String) : SyncState()
    }

    init {
        if (com.freemetar.app.BuildConfig.DRIVE_SYNC) {
            checkSignInStatus()
            setupAutoSync()
        }
    }

    /** Sprawdź czy użytkownik jest zalogowany */
    private fun checkSignInStatus() {
        val account = driveSync.getSignedInAccount()
        _isSignedIn.value = account != null

        if (account != null) {
            viewModelScope.launch {
                driveSync.initialize(account).onSuccess {
                    checkForExistingData()
                }
            }
        }
    }

    /** Sprawdź czy są dane na Drive przy pierwszym uruchomieniu */
    private fun checkForExistingData() {
        viewModelScope.launch {
            val localFlights = flightStore.load()
            val hasData = driveSync.hasExistingData()

            if (hasData && localFlights.isEmpty()) {
                // Są dane na Drive, ale lokalnie pusto - pobierz
                _syncState.value = SyncState.Syncing
                driveSync.downloadFlights().onSuccess { flights ->
                    flightStore.save(flights)
                    _syncState.value = SyncState.Success(loc("Pobrano ${flights.size} lotów z Google Drive", "Downloaded ${flights.size} flights from Google Drive"))
                }.onFailure { e ->
                    _syncState.value = SyncState.Error(loc("Błąd pobierania: ${e.message}", "Download failed: ${e.message}"))
                }
            } else if (localFlights.isNotEmpty() && !hasData) {
                // Są dane lokalnie, ale nie na Drive - wyślij
                uploadToCloud(localFlights)
            }
        }
    }

    /** Konfiguracja automatycznej synchronizacji po każdym zapisie */
    private fun setupAutoSync() {
        flightStore.driveSync = { flights ->
            if (_isSignedIn.value) {
                viewModelScope.launch {
                    uploadToCloud(flights)
                }
            }
        }
    }

    /** Obsługa rezultatu logowania Google */
    fun handleSignInResult(task: Task<GoogleSignInAccount>) {
        viewModelScope.launch {
            try {
                val account = task.getResult(ApiException::class.java)
                _syncState.value = SyncState.SigningIn

                driveSync.initialize(account).onSuccess {
                    _isSignedIn.value = true
                    _syncState.value = SyncState.Success(loc("Zalogowano pomyślnie", "Signed in"))
                    checkForExistingData()
                }.onFailure { e ->
                    _syncState.value = SyncState.Error(loc("Błąd inicjalizacji: ${e.message}", "Initialisation failed: ${e.message}"))
                }
            } catch (e: ApiException) {
                _syncState.value = SyncState.Error(signInError(e.statusCode))
            }
        }
    }

    /** Kody z GoogleSignInStatusCodes / CommonStatusCodes przełożone na zrozumiały komunikat. */
    private fun signInError(code: Int): String = when (code) {
        12501 -> loc("Logowanie anulowane", "Sign-in cancelled")
        7 -> loc("Brak połączenia z internetem", "No internet connection")
        10 -> loc("Błąd konfiguracji Google (kod 10): brak klienta OAuth dla tego podpisu aplikacji", "Google configuration error (code 10): no OAuth client for this app signature")
        12500 -> loc("Logowanie nie powiodło się (kod 12500) — sprawdź ekran zgody OAuth i konto testowe", "Sign-in failed (code 12500) — check the OAuth consent screen and the test account")
        else -> loc("Logowanie nie powiodło się (kod $code)", "Sign-in failed (code $code)")
    }

    /** Wylogowanie */
    fun signOut() {
        viewModelScope.launch {
            driveSync.signOut()
            _isSignedIn.value = false
            _syncState.value = SyncState.Success(loc("Wylogowano", "Signed out"))
        }
    }

    /** Ręczna synchronizacja - pobierz z Drive */
    fun syncFromCloud() {
        viewModelScope.launch {
            _syncState.value = SyncState.Syncing
            driveSync.downloadFlights().onSuccess { cloudFlights ->
                val localFlights = flightStore.load()
                // Scalanie: zachowaj oba zestawy, bez duplikatów
                val merged = com.freemetar.app.data.link.FlightArchive.merge(localFlights, cloudFlights)
                flightStore.save(merged)
                _syncState.value = SyncState.Success(loc("Zsynchronizowano ${cloudFlights.size} lotów", "Synced ${cloudFlights.size} flights"))
            }.onFailure { e ->
                _syncState.value = SyncState.Error(loc("Błąd: ${e.message}", "Error: ${e.message}"))
            }
        }
    }

    /** Ręczna synchronizacja - wyślij na Drive */
    fun syncToCloud() {
        viewModelScope.launch {
            val flights = flightStore.load()
            uploadToCloud(flights)
        }
    }

    /** Przywróć z backupu */
    fun restoreFromBackup() {
        viewModelScope.launch {
            _syncState.value = SyncState.Syncing
            driveSync.restoreFromBackup().onSuccess { backupFlights ->
                flightStore.save(backupFlights)
                _syncState.value = SyncState.Success(loc("Przywrócono ${backupFlights.size} lotów z backupu", "Restored ${backupFlights.size} flights from the backup"))
            }.onFailure { e ->
                _syncState.value = SyncState.Error(loc("Brak backupu: ${e.message}", "No backup: ${e.message}"))
            }
        }
    }

    private suspend fun uploadToCloud(flights: List<com.freemetar.app.data.link.FlightLog>) {
        driveSync.uploadFlights(flights).onSuccess {
            // Sukces - nie pokazuj komunikatu dla auto-sync
        }.onFailure { e ->
            _syncState.value = SyncState.Error(loc("Błąd wysyłania: ${e.message}", "Upload failed: ${e.message}"))
        }
    }

    /** Reset stanu (po pokazaniu komunikatu) */
    fun clearState() {
        _syncState.value = SyncState.Idle
    }
}
