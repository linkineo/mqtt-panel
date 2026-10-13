"""Certificate helpers for the test lab (Python, no OpenSSL binary needed).

Generates a private CA, server and client certificates (ECDSA P-256) or a
self-signed server certificate, writes PEM files and prints SHA-256
fingerprints in the form the app shows.
"""
from __future__ import annotations

import datetime as dt
import ipaddress
import os
import socket
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.serialization import pkcs12
from cryptography.x509.oid import ExtendedKeyUsageOID, NameOID

ONE_DAY = dt.timedelta(days=1)


def _name(cn: str) -> x509.Name:
    return x509.Name([
        x509.NameAttribute(NameOID.ORGANIZATION_NAME, "MQTT Panel test lab"),
        x509.NameAttribute(NameOID.COMMON_NAME, cn),
    ])


def _write(path: Path, data: bytes, private: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    if private:
        os.chmod(path, 0o600)


def _key_pem(key) -> bytes:
    return key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )


def _san(hosts: list[str]) -> x509.SubjectAlternativeName:
    names: list[x509.GeneralName] = []
    for h in hosts:
        try:
            names.append(x509.IPAddress(ipaddress.ip_address(h)))
        except ValueError:
            names.append(x509.DNSName(h))
    return x509.SubjectAlternativeName(names)


def fingerprint(cert: x509.Certificate) -> str:
    raw = cert.fingerprint(hashes.SHA256()).hex().upper()
    return ":".join(raw[i:i + 2] for i in range(0, len(raw), 2))


def default_hosts(extra: list[str] | None = None) -> list[str]:
    hosts = ["localhost", "127.0.0.1", "::1"]
    try:
        hn = socket.gethostname()
        if hn and hn not in hosts:
            hosts.append(hn)
            if "." in hn and hn.split(".")[0] not in hosts:
                hosts.append(hn.split(".")[0])
    except OSError:
        pass
    for h in extra or []:
        if h not in hosts:
            hosts.append(h)
    return hosts


class Lab:
    """A folder with ca.pem, server.pem, server-key.pem, client.pem, client-key.pem."""

    def __init__(self, out: Path):
        self.out = out
        self.ca_cert: x509.Certificate | None = None
        self.ca_key = None

    # ---- CA -------------------------------------------------------------
    def make_ca(self, name: str = "MQTT Panel test lab CA", years: int = 5):
        key = ec.generate_private_key(ec.SECP256R1())
        now = dt.datetime.now(dt.timezone.utc) - ONE_DAY
        cert = (
            x509.CertificateBuilder()
            .subject_name(_name(name))
            .issuer_name(_name(name))
            .public_key(key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(now)
            .not_valid_after(now + dt.timedelta(days=365 * years))
            .add_extension(x509.BasicConstraints(ca=True, path_length=0), critical=True)
            .add_extension(x509.KeyUsage(
                digital_signature=True, key_cert_sign=True, crl_sign=True,
                content_commitment=False, key_encipherment=False, data_encipherment=False,
                key_agreement=False, encipher_only=False, decipher_only=False), critical=True)
            .add_extension(x509.SubjectKeyIdentifier.from_public_key(key.public_key()), critical=False)
            .sign(key, hashes.SHA256())
        )
        _write(self.out / "ca.pem", cert.public_bytes(serialization.Encoding.PEM))
        _write(self.out / "ca-key.pem", _key_pem(key), private=True)
        self.ca_cert, self.ca_key = cert, key
        return cert

    def _signed(self, cn: str, server: bool, hosts: list[str] | None, days: int):
        assert self.ca_cert is not None and self.ca_key is not None, "make_ca() first"
        key = ec.generate_private_key(ec.SECP256R1())
        now = dt.datetime.now(dt.timezone.utc) - ONE_DAY
        b = (
            x509.CertificateBuilder()
            .subject_name(_name(cn))
            .issuer_name(self.ca_cert.subject)
            .public_key(key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(now)
            .not_valid_after(now + dt.timedelta(days=days))
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.KeyUsage(
                digital_signature=True, key_encipherment=True, key_cert_sign=False, crl_sign=False,
                content_commitment=False, data_encipherment=False, key_agreement=False,
                encipher_only=False, decipher_only=False), critical=True)
            .add_extension(x509.ExtendedKeyUsage(
                [ExtendedKeyUsageOID.SERVER_AUTH if server else ExtendedKeyUsageOID.CLIENT_AUTH]),
                critical=False)
            .add_extension(x509.AuthorityKeyIdentifier.from_issuer_public_key(self.ca_key.public_key()),
                           critical=False)
        )
        if server:
            b = b.add_extension(_san(hosts or default_hosts()), critical=False)
        cert = b.sign(self.ca_key, hashes.SHA256())
        return cert, key

    def make_server(self, hosts: list[str] | None = None, days: int = 730):
        cert, key = self._signed("MQTT Panel test lab broker", True, hosts, days)
        _write(self.out / "server.pem", cert.public_bytes(serialization.Encoding.PEM))
        _write(self.out / "server-key.pem", _key_pem(key), private=True)
        return cert

    def make_client(self, name: str = "lab-client", days: int = 730, p12_password: str | None = None):
        cert, key = self._signed(name, False, None, days)
        _write(self.out / "client.pem", cert.public_bytes(serialization.Encoding.PEM))
        _write(self.out / "client-key.pem", _key_pem(key), private=True)
        if p12_password:
            data = pkcs12.serialize_key_and_certificates(
                name.encode(), key, cert, [self.ca_cert] if self.ca_cert else None,
                serialization.BestAvailableEncryption(p12_password.encode()))
            _write(self.out / "client.p12", data, private=True)
        return cert

    def make_self_signed_server(self, hosts: list[str] | None = None, days: int = 730):
        """A server certificate signed by its own key, no CA: the app must trust it by fingerprint."""
        key = ec.generate_private_key(ec.SECP256R1())
        now = dt.datetime.now(dt.timezone.utc) - ONE_DAY
        name = _name("MQTT Panel test lab self-signed broker")
        cert = (
            x509.CertificateBuilder()
            .subject_name(name).issuer_name(name)
            .public_key(key.public_key())
            .serial_number(x509.random_serial_number())
            .not_valid_before(now)
            .not_valid_after(now + dt.timedelta(days=days))
            .add_extension(x509.BasicConstraints(ca=False, path_length=None), critical=True)
            .add_extension(x509.ExtendedKeyUsage([ExtendedKeyUsageOID.SERVER_AUTH]), critical=False)
            .add_extension(_san(hosts or default_hosts()), critical=False)
            .sign(key, hashes.SHA256())
        )
        _write(self.out / "server.pem", cert.public_bytes(serialization.Encoding.PEM))
        _write(self.out / "server-key.pem", _key_pem(key), private=True)
        return cert


def pkcs12_to_pem(p12_path: Path, password: str | None, out_dir: Path) -> tuple[Path, Path, Path | None]:
    """Split a .p12/.pfx (as exported by the app) into client.pem, client-key.pem and, if present, ca.pem."""
    key, cert, extra = pkcs12.load_key_and_certificates(
        p12_path.read_bytes(), password.encode() if password else None)
    if key is None or cert is None:
        raise SystemExit(f"{p12_path}: no key or certificate inside")
    cert_path, key_path = out_dir / "client.pem", out_dir / "client-key.pem"
    _write(cert_path, cert.public_bytes(serialization.Encoding.PEM))
    _write(key_path, _key_pem(key), private=True)
    ca_path = None
    if extra:
        ca_path = out_dir / "ca.pem"
        _write(ca_path, b"".join(c.public_bytes(serialization.Encoding.PEM) for c in extra))
    return cert_path, key_path, ca_path
