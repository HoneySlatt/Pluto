package app.pluto.mobile

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Which servers Android 17 counts as the home network (LocalNetwork.kt).
class LocalNetworkTest {
    @Test
    fun privateAddressesAndLocalNamesAreLocal() {
        for (server in listOf(
            "http://192.168.1.74:3774",
            "http://10.0.0.2:3774/",
            "http://172.20.1.1:3774",
            "http://169.254.3.4",
            "http://[fd00::1]:3774",
            "http://[fe80::1]:3774",
            "http://pluto.local:3774",
            "http://desktop:3774",
        )) assertTrue(server, LocalNetwork.isLocal(server))
    }

    @Test
    fun publicAndLoopbackAddressesAreNot() {
        for (server in listOf(
            "https://pluto.example.com",
            "http://100.101.102.103:3774",
            "http://8.8.8.8",
            "http://172.32.0.1",
            "http://localhost:3774",
        )) assertFalse(server, LocalNetwork.isLocal(server))
    }
}
