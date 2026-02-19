import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';
import * as net from 'net';
import lockfile from 'proper-lockfile';
import pidUsage from 'pidusage';

interface ServiceRegistration {
    port: number;
    pid: number;
    started: string;
}

interface Registry {
    [serviceId: string]: ServiceRegistration;
}

/**
 * Service registry for service discovery with file locking.
 *
 * Provides a JSON-based registry for workers to register themselves and
 * for parents to discover running services. Supports concurrent access
 * via file locking.
 */
export class ServiceRegistry {
    private static REGISTRY_PATH = path.join(os.homedir(), '.multifrost', 'services.json');

    private static async ensureRegistryDir(): Promise<void> {
        const dir = path.dirname(ServiceRegistry.REGISTRY_PATH);
        await fs.mkdir(dir, { recursive: true });
    }

    private static async ensureRegistryFile(): Promise<void> {
        await ServiceRegistry.ensureRegistryDir();
        try {
            await fs.access(ServiceRegistry.REGISTRY_PATH);
        } catch {
            // File doesn't exist, create empty registry
            await fs.writeFile(ServiceRegistry.REGISTRY_PATH, '{}');
        }
    }

    private static async readRegistry(): Promise<Registry> {
        await ServiceRegistry.ensureRegistryFile();
        try {
            const data = await fs.readFile(ServiceRegistry.REGISTRY_PATH, 'utf-8');
            return JSON.parse(data);
        } catch (error) {
            if (error instanceof Error && 'code' in error && error.code === 'ENOENT') {
                return {};
            }
            throw error;
        }
    }

    private static async writeRegistry(data: Registry): Promise<void> {
        await ServiceRegistry.ensureRegistryDir();
        // Write directly to the file (not temp + rename) to avoid lockfile issues
        await fs.writeFile(ServiceRegistry.REGISTRY_PATH, JSON.stringify(data, null, 2));
    }

    private static async isProcessAlive(pid: number): Promise<boolean> {
        try {
            const stats = await pidUsage(pid);
            return stats !== null;
        } catch {
            return false;
        }
    }

    /**
     * Register service, enforce uniqueness.
     * @param serviceId The unique service identifier
     * @returns The port number assigned to the service
     * @throws Error if service_id already running
     */
    static async register(serviceId: string): Promise<number> {
        // Ensure directory and file exist before locking
        await ServiceRegistry.ensureRegistryFile();
        const release = await lockfile.lock(ServiceRegistry.REGISTRY_PATH);
        try {
            const registry = await ServiceRegistry.readRegistry();

            // Check if service_id already exists with live PID
            if (serviceId in registry) {
                const existing = registry[serviceId];
                const alive = await ServiceRegistry.isProcessAlive(existing.pid);
                if (alive) {
                    throw new Error(
                        `Service '${serviceId}' already running (PID: ${existing.pid}, port: ${existing.port})`
                    );
                }
                // Dead process - will be overwritten
            }

            // Pick free port
            const port = await ServiceRegistry.findFreePort();

            // Register
            registry[serviceId] = {
                port,
                pid: process.pid,
                started: new Date().toISOString(),
            };

            await ServiceRegistry.writeRegistry(registry);
            return port;
        } finally {
            await release();
        }
    }

    /**
     * Discover service by ID, with polling.
     * @param serviceId The service identifier to discover
     * @param timeout Timeout in milliseconds (default: 5000)
     * @returns The port number of the discovered service
     * @throws Error if service not found within timeout
     */
    static async discover(serviceId: string, timeout: number = 5000): Promise<number> {
        const deadline = Date.now() + timeout;

        while (Date.now() < deadline) {
            // Ensure directory and file exist before locking
            await ServiceRegistry.ensureRegistryFile();
            const release = await lockfile.lock(ServiceRegistry.REGISTRY_PATH);
            try {
                const registry = await ServiceRegistry.readRegistry();
                const reg = registry[serviceId];

                if (reg && await ServiceRegistry.isProcessAlive(reg.pid)) {
                    return reg.port;
                }
            } finally {
                await release();
            }

            await new Promise(resolve => setTimeout(resolve, 100));
        }

        throw new Error(`Service '${serviceId}' not found within ${timeout}ms`);
    }

    /**
     * Unregister service (called by process on shutdown).
     * @param serviceId The service identifier to unregister
     */
    static async unregister(serviceId: string): Promise<void> {
        // Ensure directory and file exist before locking
        await ServiceRegistry.ensureRegistryFile();
        const release = await lockfile.lock(ServiceRegistry.REGISTRY_PATH);
        try {
            const registry = await ServiceRegistry.readRegistry();
            if (serviceId in registry) {
                const reg = registry[serviceId];
                // Only remove if it's our PID
                if (reg.pid === process.pid) {
                    delete registry[serviceId];
                    await ServiceRegistry.writeRegistry(registry);
                }
            }
        } finally {
            await release();
        }
    }

    /**
     * Find a free port for ZeroMQ communication.
     * @returns A free port number
     */
    static async findFreePort(): Promise<number> {
        return new Promise((resolve, reject) => {
            const server = net.createServer();
            server.listen(0);
            server.once('listening', () => {
                const address = server.address();
                const port = address && typeof address === 'object' ? address.port : 5555;
                server.close(() => resolve(port));
            });
            server.once('error', reject);
        });
    }
}
