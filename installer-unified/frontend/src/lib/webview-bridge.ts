/**
 * WebView Bridge for Tauri
 * Replaces WebView2 bridge with Tauri event system
 * 
 * This file should be replaced when copying the actual React UI from ui/cadalytix-ui/
 * Only the event listening needs to change, all event types remain the same
 */

import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { emit } from '@tauri-apps/api/event';

/**
 * Tauri-based bridge to replace WebView2 bridge
 * Provides the same interface as the original webview-bridge.ts
 */
export class TauriBridge {
  private listeners: Map<string, UnlistenFn[]> = new Map();

  /**
   * Listen to an event from the Rust backend
   */
  async on<T = any>(eventName: string, callback: (data: T) => void): Promise<void> {
    const unlisten = await listen<T>(eventName, (event) => {
      callback(event.payload);
    });
    
    if (!this.listeners.has(eventName)) {
      this.listeners.set(eventName, []);
    }
    this.listeners.get(eventName)!.push(unlisten);
  }

  /**
   * Emit an event to the Rust backend
   */
  async emit<T = any>(eventName: string, data?: T): Promise<void> {
    await emit(eventName, data);
  }

  /**
   * Remove all listeners for an event
   */
  off(eventName: string): void {
    const unlisteners = this.listeners.get(eventName);
    if (unlisteners) {
      unlisteners.forEach((unlisten) => unlisten());
      this.listeners.delete(eventName);
    }
  }

  /**
   * Remove all listeners
   */
  removeAllListeners(): void {
    this.listeners.forEach((unlisteners) => {
      unlisteners.forEach((unlisten) => unlisten());
    });
    this.listeners.clear();
  }
}

// Export singleton instance
export const webviewBridge = new TauriBridge();

// For backward compatibility with existing code
export default webviewBridge;

