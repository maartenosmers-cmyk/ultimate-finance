package finance

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

object Palette {
    val Emerald = Color(0xFF1AC878)
    val EmeraldDim = Color(0xFF128A54)
    val Red = Color(0xFFF2595E)
    val SurfaceDark = Color(0xFF101418)
    val SurfaceCardDark = Color(0xFF181E24)
}

private fun financeDark() = darkColorScheme(
    primary = Palette.Emerald,
    onPrimary = Color.Black,
    secondary = Palette.EmeraldDim,
    background = Palette.SurfaceDark,
    surface = Palette.SurfaceDark,
    surfaceVariant = Palette.SurfaceCardDark,
    error = Palette.Red,
)

private fun financeLight() = lightColorScheme(
    primary = Palette.EmeraldDim,
    onPrimary = Color.White,
    secondary = Palette.Emerald,
    error = Palette.Red,
)

@Composable
fun FinanceTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = financeDark(),
        content = content,
    )
}
