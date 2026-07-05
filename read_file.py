import sys

def main():
    if len(sys.argv) < 2:
        print("Usage: python read_file.py <file>")
        return
    with open(sys.argv[1], 'r') as f:
        print(f.read())

if __name__ == "__main__":
    main()
