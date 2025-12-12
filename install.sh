#!/bin/bash
set -e

echo "Building keyboard-middleware..."
cargo build --release

echo "Installing binary..."
sudo cp target/release/keyboard-middleware /usr/local/bin/

echo "Installing systemd service..."
sudo cp keyboard-middleware.service /etc/systemd/system/

echo "Reloading systemd..."
sudo systemctl daemon-reload

echo "Enabling service..."
sudo systemctl enable keyboard-middleware

echo ""
echo "Installation complete!"
echo ""
echo "To start the service now:"
echo "  sudo systemctl start keyboard-middleware"
echo ""
echo "To view logs:"
echo "  sudo journalctl -u keyboard-middleware -f"
echo ""
echo "To stop the service:"
echo "  sudo systemctl stop keyboard-middleware"
