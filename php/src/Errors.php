<?php

declare(strict_types=1);

namespace Multifrost;

/**
 * Error origin constants.
 */
enum ErrorOrigin: string
{
    case Transport = 'transport';
    case Bootstrap = 'bootstrap';
    case Registration = 'registration';
    case Router = 'router';
    case Protocol = 'protocol';
    case Remote = 'remote';
    case Application = 'application';
}

/**
 * Base exception for all Multifrost errors.
 */
abstract class MultifrostException extends \RuntimeException
{
    public function __construct(
        string $message = '',
        int $code = 0,
        ?\Throwable $previous = null,
    ) {
        parent::__construct($message, $code, $previous);
    }
}

final class TransportError extends MultifrostException
{
    public function __construct(
        string $message = '',
        ?\Throwable $previous = null,
    ) {
        parent::__construct(
            \sprintf('transport failure [%s]: %s', ErrorOrigin::Transport->value, $message),
            0,
            $previous,
        );
    }
}

final class BootstrapError extends MultifrostException
{
    public function __construct(
        string $message = '',
        ?\Throwable $previous = null,
    ) {
        parent::__construct(
            \sprintf('bootstrap failure [%s]: %s', ErrorOrigin::Bootstrap->value, $message),
            0,
            $previous,
        );
    }
}

final class RegistrationError extends MultifrostException
{
    public function __construct(
        string $reason = '',
        ?\Throwable $previous = null,
    ) {
        parent::__construct(
            \sprintf('registration failure [%s]: %s', ErrorOrigin::Registration->value, $reason),
            0,
            $previous,
        );
    }
}

final class RouterError extends MultifrostException
{
    public function __construct(
        public readonly string $errorCode,
        string $message = '',
        ?\Throwable $previous = null,
    ) {
        parent::__construct(
            \sprintf('router error [%s] %s: %s', ErrorOrigin::Router->value, $errorCode, $message),
            0,
            $previous,
        );
    }
}

final class RemoteCallError extends MultifrostException
{
    public function __construct(
        string $message = '',
        ?\Throwable $previous = null,
    ) {
        parent::__construct(
            \sprintf('remote call failure [%s]: %s', ErrorOrigin::Remote->value, $message),
            0,
            $previous,
        );
    }
}

// errorFromWire() function is defined in src/functions.php (loaded via composer "files" autoload)
