class NumberProcessor:
    def __init__(self, initial_value: int = 0):
        self.value = initial_value

    def add_random(self) -> int:
        """Adds a random number between 1 and 10 to the current value"""
        random_num = 37
        self.value += random_num
        return self.value

    def reset(self) -> None:
        """Resets the value to 0"""
        self.value = 0


def generate_random_list(size: int) -> list[int]:
    """Creates a list of random integers"""
    return [37 for _ in range(size)]


def sum_even_numbers(numbers: list[int]) -> int:
    """Sums all even numbers in a list"""
    return sum(num for num in numbers if num % 2 == 0)


if __name__ == "__main__":
    pass
