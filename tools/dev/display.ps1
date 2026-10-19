# 把窗口搬到指定显示器的工具函数（dot-source 使用）。
#
# 背景：本机有两块显示器，显示器 1 是用户工作用的，显示器 2 是测试用的。
# 任何会**显示窗口**的进程（mir-client、可见的控制台）都应固定到显示器 2，
# 绝不能弹到显示器 1 上打断用户。
#
# 用法：
#   . "$PSScriptRoot\display.ps1"
#   Wait-WindowOnDisplay -ProcessId $p.Id -Display 2 -TimeoutSeconds 10

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
    public const int SW_RESTORE = 9;
}
"@

<#
.SYNOPSIS
    返回第 N 块显示器的工作区（1 起），找不到返回 $null。
#>
function Get-DisplayWorkArea {
    param([int]$Display = 2)
    $screens = [System.Windows.Forms.Screen]::AllScreens | Sort-Object { $_.Bounds.X }
    if ($Display -lt 1 -or $Display -gt $screens.Count) { return $null }
    return $screens[$Display - 1].WorkingArea
}

<#
.SYNOPSIS
    等进程主窗口出现，然后把它搬到指定显示器的左上角。
.DESCRIPTION
    窗口还没创建时会轮询 MainWindowHandle；超时返回 $false（不抛异常，调用方自行决定）。
    隐藏启动的进程（MainWindowHandle 恒为 0）不会做任何事，直接返回 $false。
#>
function Wait-WindowOnDisplay {
    param(
        [Parameter(Mandatory = $true)][int]$ProcessId,
        [int]$Display = 2,
        [int]$TimeoutSeconds = 10,
        [switch]$Resize
    )
    $area = Get-DisplayWorkArea -Display $Display
    if (-not $area) { return $false }

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
                    $flags -bor 0x0001) | Out-Null # SWP_NOSIZE
            }
            Write-Host "窗口已移到显示器 $Display（进程 $ProcessId）"
            return $true
        }
        Start-Sleep -Milliseconds 200
    }
    return $false
}
