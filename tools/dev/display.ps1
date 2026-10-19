# 把窗口搬到指定显示器的工具函数（dot-source 使用）。
#
# 背景：本机可能接两块显示器，显示器 1 是用户工作用的，显示器 2 是测试用的。
# 但**显示器 2 可能开、也可能关**：
#   * 显示器 2 开着  → 窗口放显示器 2；
#   * 显示器 2 关了  → 退回到显示器 1（否则窗口会跑到不存在的坐标上，用户看不到）。
#
# 用法（`-Display 0` = 自动，推荐）：
#   . "$PSScriptRoot\display.ps1"
#   Wait-WindowOnDisplay -ProcessId $p.Id -Display 0
# 也可以显式指定 1/2；指定的显示器不存在时自动退回自动选择。

Add-Type -AssemblyName System.Windows.Forms | Out-Null

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win32Window {
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter,
        int X, int Y, int cx, int cy, uint uFlags);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    public const uint SWP_NOZORDER = 0x0004;
    public const uint SWP_NOACTIVATE = 0x0010;
    public const uint SWP_SHOWWINDOW = 0x0040;
    public const uint SWP_NOSIZE = 0x0001;
    public const int SW_RESTORE = 9;
}
"@

<#
.SYNOPSIS
    当前所有**活动**显示器，按左上角 X 排序（显示器关掉后不会出现在这里）。
#>
function Get-ActiveScreens {
    return [System.Windows.Forms.Screen]::AllScreens | Sort-Object { $_.Bounds.X }
}

<#
.SYNOPSIS
    选一个放窗口的显示器：`Display 0` = 自动。
.DESCRIPTION
    自动规则：有第二块（非主）显示器就用它（测试用），否则用主显示器。
    显式指定 1/2 但该显示器不存在时，同样退回自动规则。
#>
function Get-TargetScreen {
    param([int]$Display = 0)
    $screens = @(Get-ActiveScreens)
    if ($screens.Count -eq 0) { return $null }

    if ($Display -ge 1 -and $Display -le $screens.Count) {
        return $screens[$Display - 1]
    }
    # 自动：优先非主显示器（测试用），没有就主显示器。
    $secondary = $screens | Where-Object { -not $_.Primary } | Select-Object -First 1
    if ($secondary) { return $secondary }
    return $screens | Where-Object { $_.Primary } | Select-Object -First 1
}

<#
.SYNOPSIS
    返回目标显示器的工作区（`Display 0` = 自动），找不到返回 $null。
#>
function Get-DisplayWorkArea {
    param([int]$Display = 0)
    $screen = Get-TargetScreen -Display $Display
    if (-not $screen) { return $null }
    return $screen.WorkingArea
}

<#
.SYNOPSIS
    等进程主窗口出现，然后把它搬到目标显示器（默认自动：有副屏用副屏，否则主屏）的左上角。
.DESCRIPTION
    窗口还没创建时会轮询 MainWindowHandle；超时返回 $false（不抛异常，调用方自行决定）。
    隐藏启动的进程（MainWindowHandle 恒为 0）不会做任何事，直接返回 $false。
#>
function Wait-WindowOnDisplay {
    param(
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [int]$Display = 0,
        [int]$TimeoutSeconds = 10,
        [switch]$Resize
    )
    $screen = Get-TargetScreen -Display $Display
    if (-not $screen) { return $false }
    $area = $screen.WorkingArea

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if (-not $process) { return $false }
        $handle = $process.MainWindowHandle
        if ($handle -ne [IntPtr]::Zero) {
            $flags = [Win32Window]::SWP_NOZORDER -bor [Win32Window]::SWP_SHOWWINDOW
            [Win32Window]::ShowWindow($handle, [Win32Window]::SW_RESTORE) | Out-Null
            if ($Resize) {
                [Win32Window]::SetWindowPos($handle, [IntPtr]::Zero,
                    $area.X, $area.Y, $area.Width, $area.Height, $flags) | Out-Null
            } else {
                [Win32Window]::SetWindowPos($handle, [IntPtr]::Zero,
                    $area.X, $area.Y, 0, 0,
                    $flags -bor [Win32Window]::SWP_NOSIZE) | Out-Null
            }
            Write-Host ("窗口已移到 {0}（进程 {1}）" -f $screen.DeviceName, $ProcessId)
            return $true
        }
        Start-Sleep -Milliseconds 200
    }
    return $false
}
